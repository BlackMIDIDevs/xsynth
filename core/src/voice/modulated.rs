//! Voice generators whose values follow live MIDI controllers.
//!
//! These replace the constant gain/pan/pitch stages of a voice. When a region
//! has no controller-driven modulation they cost the same as a constant: the
//! value is only recomputed when the channel broadcasts a control change.

use std::sync::Arc;

use simdeez::prelude::*;
use xsynth_soundfonts::modulation::{
    EnvelopeTarget, ModDestination, RegionModulation, RuntimeModulator,
};

use crate::{
    helpers::db_to_amp,
    voice::{
        ControllerState, EnvelopeDescriptor, ReleaseType, SIMDSampleMono, SIMDSampleStereo,
        SIMDVoiceGenerator, VoiceControlData, VoiceGeneratorBase,
    },
};

#[inline]
fn eval_modulator(m: &RuntimeModulator, c: &ControllerState) -> f32 {
    let mut value = m.amount * m.source.shape(c.normalized(m.source.controller));
    if let Some(src) = m.amount_source {
        value *= src.shape(c.normalized(src.controller));
    }
    if m.absolute {
        value.abs()
    } else {
        value
    }
}

/// Sums the modulators of a region that target the given destination.
#[inline]
pub(crate) fn modulation_sum(
    modulation: &RegionModulation,
    c: &ControllerState,
    dest: impl Fn(&ModDestination) -> bool,
) -> f32 {
    modulation
        .modulators
        .iter()
        .filter(|m| dest(&m.destination))
        .map(|m| eval_modulator(m, c))
        .sum()
}

#[inline]
pub(crate) fn cents_factor(cents: f32) -> f32 {
    2.0f32.powf(cents / 1200.0)
}

/// Applies the note-on envelope modulators of a region to an envelope.
pub(crate) fn modulate_envelope(
    mut env: EnvelopeDescriptor,
    modulation: &RegionModulation,
    c: &ControllerState,
) -> EnvelopeDescriptor {
    for m in modulation.modulators.iter() {
        let value = match m.destination {
            ModDestination::EnvelopeLinear(_) | ModDestination::EnvelopeLog(_) => {
                eval_modulator(m, c)
            }
            _ => continue,
        };
        match m.destination {
            ModDestination::EnvelopeLinear(target) => match target {
                EnvelopeTarget::Start => {
                    env.start_percent = (env.start_percent + value / 100.0).clamp(0.0, 1.0)
                }
                EnvelopeTarget::Delay => env.delay = (env.delay + value).max(0.0),
                EnvelopeTarget::Attack => env.attack = (env.attack + value).max(0.0),
                EnvelopeTarget::Hold => env.hold = (env.hold + value).max(0.0),
                EnvelopeTarget::Decay => env.decay = (env.decay + value).max(0.0),
                EnvelopeTarget::Sustain => {
                    env.sustain_percent = (env.sustain_percent + value / 100.0).clamp(0.0, 1.0)
                }
                EnvelopeTarget::Release => env.release = (env.release + value).max(0.0),
            },
            ModDestination::EnvelopeLog(target) => {
                let factor = cents_factor(value);
                match target {
                    EnvelopeTarget::Start => {}
                    EnvelopeTarget::Delay => env.delay *= factor,
                    EnvelopeTarget::Attack => env.attack *= factor,
                    EnvelopeTarget::Hold => env.hold *= factor,
                    EnvelopeTarget::Decay => env.decay *= factor,
                    EnvelopeTarget::Sustain => {
                        env.sustain_percent =
                            (env.sustain_percent * 10f32.powf(-value / 200.0)).clamp(0.0, 1.0)
                    }
                    EnvelopeTarget::Release => env.release *= factor,
                }
            }
            _ => {}
        }
    }
    env
}

/// Controller-driven gain and pan of a voice.
#[derive(Clone)]
pub(crate) struct GainModulation {
    modulation: Arc<RegionModulation>,
    has_amplitude: bool,
    has_volume: bool,
    has_pan: bool,
}

impl GainModulation {
    pub fn new(modulation: Arc<RegionModulation>) -> Self {
        let has_amplitude = modulation.has_destination(|d| *d == ModDestination::AmplitudePercent);
        let has_volume = modulation.has_destination(|d| *d == ModDestination::VolumeDb);
        let has_pan = modulation.has_destination(|d| *d == ModDestination::Pan);
        Self {
            modulation,
            has_amplitude,
            has_volume,
            has_pan,
        }
    }

    /// Whether the modulation changes the gain or pan at all.
    pub fn is_active(&self) -> bool {
        self.has_amplitude
            || self.has_volume
            || self.has_pan
            || !self.modulation.crossfades.is_empty()
    }

    fn gain(&self, c: &ControllerState) -> f32 {
        let m = &*self.modulation;
        let mut gain = 1.0;
        if self.has_amplitude {
            let amp = m.amplitude_percent
                + modulation_sum(m, c, |d| *d == ModDestination::AmplitudePercent);
            gain *= amp.clamp(0.0, 100.0) / 100.0;
        }
        if self.has_volume {
            gain *= db_to_amp(modulation_sum(m, c, |d| *d == ModDestination::VolumeDb));
        }
        for xf in m.crossfades.iter() {
            gain *= xf.gain(c.cc[xf.cc as usize], m.crossfade_curve);
        }
        gain
    }

    fn pan_offset(&self, c: &ControllerState) -> f32 {
        if self.has_pan {
            modulation_sum(&self.modulation, c, |d| *d == ModDestination::Pan) * 0.5
        } else {
            0.0
        }
    }
}

/// Pan law used by the sample voices: 0.0 = left, 0.5 = center, 1.0 = right.
#[inline]
pub(crate) fn pan_gains(pan: f32) -> (f32, f32) {
    let pan = pan.clamp(0.0, 1.0) * std::f32::consts::PI / 2.0;
    ((pan.cos() * 1.42).min(1.0), (pan.sin() * 1.42).min(1.0))
}

/// A short linear ramp, stepped once per SIMD frame, that removes zipper noise
/// when a controller changes the gain of a playing voice.
#[derive(Clone, Copy)]
struct Ramp {
    current: f32,
    target: f32,
    step: f32,
    remaining: u32,
}

impl Ramp {
    fn new(value: f32) -> Self {
        Self {
            current: value,
            target: value,
            step: 0.0,
            remaining: 0,
        }
    }

    fn set(&mut self, target: f32, frames: u32) {
        if target != self.target {
            self.target = target;
            self.remaining = frames.max(1);
            self.step = (target - self.current) / self.remaining as f32;
        }
    }

    #[inline(always)]
    fn next(&mut self) -> f32 {
        self.remaining -= 1;
        if self.remaining == 0 {
            self.current = self.target;
        } else {
            self.current += self.step;
        }
        self.current
    }
}

/// Number of SIMD frames used to smooth controller-driven gain changes (~5ms).
pub(crate) fn gain_ramp_frames<S: Simd>(sample_rate: u32) -> u32 {
    ((sample_rate as f32 * 0.005) / S::Vf32::WIDTH as f32).max(1.0) as u32
}

/// Stereo gain stage: velocity/volume, pan and controller modulation.
pub struct SIMDVoiceGainStereo<S: Simd> {
    left: S::Vf32,
    right: S::Vf32,
    ramp_left: Ramp,
    ramp_right: Ramp,
    ramp_frames: u32,
    amp: f32,
    pan: f32,
    modulation: Option<GainModulation>,
}

impl<S: Simd> SIMDVoiceGainStereo<S> {
    pub(crate) fn new(
        amp: f32,
        pan: f32,
        modulation: Option<GainModulation>,
        control: &VoiceControlData,
        ramp_frames: u32,
    ) -> Self {
        let modulation = modulation.filter(|m| m.is_active());
        let (l, r) = Self::compute(amp, pan, modulation.as_ref(), &control.controllers);
        simd_invoke!(S, {
            Self {
                left: S::Vf32::set1(l),
                right: S::Vf32::set1(r),
                ramp_left: Ramp::new(l),
                ramp_right: Ramp::new(r),
                ramp_frames,
                amp,
                pan,
                modulation,
            }
        })
    }

    fn compute(
        amp: f32,
        pan: f32,
        modulation: Option<&GainModulation>,
        c: &ControllerState,
    ) -> (f32, f32) {
        let (amp, pan) = match modulation {
            Some(m) => (amp * m.gain(c), pan + m.pan_offset(c)),
            None => (amp, pan),
        };
        let (l, r) = pan_gains(pan);
        (l * amp, r * amp)
    }
}

impl<S: Simd> VoiceGeneratorBase for SIMDVoiceGainStereo<S> {
    #[inline(always)]
    fn ended(&self) -> bool {
        false
    }

    #[inline(always)]
    fn signal_release(&mut self, _rel_type: ReleaseType) {}

    #[inline(always)]
    fn process_controls(&mut self, control: &VoiceControlData) {
        if let Some(m) = &self.modulation {
            let (l, r) = Self::compute(self.amp, self.pan, Some(m), &control.controllers);
            self.ramp_left.set(l, self.ramp_frames);
            self.ramp_right.set(r, self.ramp_frames);
        }
    }
}

impl<S: Simd> SIMDVoiceGenerator<S, SIMDSampleStereo<S>> for SIMDVoiceGainStereo<S> {
    #[inline(always)]
    fn next_sample(&mut self) -> SIMDSampleStereo<S> {
        if self.ramp_left.remaining > 0 {
            let l = self.ramp_left.next();
            let r = self.ramp_right.next();
            simd_invoke!(S, {
                self.left = S::Vf32::set1(l);
                self.right = S::Vf32::set1(r);
            })
        }
        SIMDSampleStereo(self.left, self.right)
    }
}

/// Mono gain stage: velocity/volume and controller modulation.
pub struct SIMDVoiceGainMono<S: Simd> {
    value: S::Vf32,
    ramp: Ramp,
    ramp_frames: u32,
    amp: f32,
    modulation: Option<GainModulation>,
}

impl<S: Simd> SIMDVoiceGainMono<S> {
    pub(crate) fn new(
        amp: f32,
        modulation: Option<GainModulation>,
        control: &VoiceControlData,
        ramp_frames: u32,
    ) -> Self {
        let modulation = modulation.filter(|m| m.is_active());
        let value = Self::compute(amp, modulation.as_ref(), &control.controllers);
        simd_invoke!(S, {
            Self {
                value: S::Vf32::set1(value),
                ramp: Ramp::new(value),
                ramp_frames,
                amp,
                modulation,
            }
        })
    }

    fn compute(amp: f32, modulation: Option<&GainModulation>, c: &ControllerState) -> f32 {
        match modulation {
            Some(m) => amp * m.gain(c),
            None => amp,
        }
    }
}

impl<S: Simd> VoiceGeneratorBase for SIMDVoiceGainMono<S> {
    #[inline(always)]
    fn ended(&self) -> bool {
        false
    }

    #[inline(always)]
    fn signal_release(&mut self, _rel_type: ReleaseType) {}

    #[inline(always)]
    fn process_controls(&mut self, control: &VoiceControlData) {
        if let Some(m) = &self.modulation {
            let value = Self::compute(self.amp, Some(m), &control.controllers);
            self.ramp.set(value, self.ramp_frames);
        }
    }
}

impl<S: Simd> SIMDVoiceGenerator<S, SIMDSampleMono<S>> for SIMDVoiceGainMono<S> {
    #[inline(always)]
    fn next_sample(&mut self) -> SIMDSampleMono<S> {
        if self.ramp.remaining > 0 {
            let v = self.ramp.next();
            simd_invoke!(S, {
                self.value = S::Vf32::set1(v);
            })
        }
        SIMDSampleMono(self.value)
    }
}

/// Pitch (playback speed) stage: base speed, channel pitch (bend/tuning/vibrato),
/// controller-driven pitch modulation and portamento.
pub struct SIMDVoicePitch<S: Simd> {
    value: S::Vf32,
    base: f32,
    channel_mult: f32,
    mod_mult: f32,
    modulation: Option<Arc<RegionModulation>>,
    glide_cents: f32,
    glide_step: f32,
}

impl<S: Simd> SIMDVoicePitch<S> {
    /// - `glide`: optional portamento as (starting offset in cents, duration in samples)
    pub(crate) fn new(
        base: f32,
        modulation: Option<Arc<RegionModulation>>,
        control: &VoiceControlData,
        glide: Option<(f32, f32)>,
    ) -> Self {
        let modulation =
            modulation.filter(|m| m.has_destination(|d| *d == ModDestination::PitchCents));
        let (glide_cents, glide_step) = match glide {
            Some((cents, samples)) if cents != 0.0 && samples >= 1.0 => {
                let frames = samples / S::Vf32::WIDTH as f32;
                (cents, cents.abs() / frames.max(1.0))
            }
            _ => (0.0, 0.0),
        };
        let mut pitch = simd_invoke!(S, {
            Self {
                value: S::Vf32::set1(base),
                base,
                channel_mult: control.voice_pitch_multiplier,
                mod_mult: 1.0,
                modulation,
                glide_cents,
                glide_step,
            }
        });
        pitch.update_modulation(&control.controllers);
        pitch.refresh();
        pitch
    }

    fn update_modulation(&mut self, c: &ControllerState) {
        if let Some(m) = &self.modulation {
            self.mod_mult =
                cents_factor(modulation_sum(m, c, |d| *d == ModDestination::PitchCents));
        }
    }

    #[inline(always)]
    fn refresh(&mut self) {
        let mut value = self.base * self.channel_mult * self.mod_mult;
        if self.glide_cents != 0.0 {
            value *= cents_factor(self.glide_cents);
        }
        simd_invoke!(S, {
            self.value = S::Vf32::set1(value);
        })
    }
}

impl<S: Simd> VoiceGeneratorBase for SIMDVoicePitch<S> {
    #[inline(always)]
    fn ended(&self) -> bool {
        false
    }

    #[inline(always)]
    fn signal_release(&mut self, _rel_type: ReleaseType) {}

    #[inline(always)]
    fn process_controls(&mut self, control: &VoiceControlData) {
        self.channel_mult = control.voice_pitch_multiplier;
        self.update_modulation(&control.controllers);
        self.refresh();
    }
}

impl<S: Simd> SIMDVoiceGenerator<S, SIMDSampleMono<S>> for SIMDVoicePitch<S> {
    #[inline(always)]
    fn next_sample(&mut self) -> SIMDSampleMono<S> {
        if self.glide_cents != 0.0 {
            if self.glide_cents > 0.0 {
                self.glide_cents = (self.glide_cents - self.glide_step).max(0.0);
            } else {
                self.glide_cents = (self.glide_cents + self.glide_step).min(0.0);
            }
            self.refresh();
        }
        SIMDSampleMono(self.value)
    }
}
