//! Voice generators whose values follow live MIDI controllers.
//!
//! These replace the constant gain/pan/pitch stages of a voice. When a region
//! has no controller-driven modulation they cost the same as a constant: the
//! value is only recomputed when the channel broadcasts a control change.

use std::{marker::PhantomData, sync::Arc};

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

/// Controller-driven state of a gain stage, only allocated for voices whose
/// gain follows controllers so that plain voices stay small.
struct GainDynamics {
    ramp_left: Ramp,
    ramp_right: Ramp,
    ramp_frames: u32,
    amp: f32,
    pan: f32,
    modulation: GainModulation,
}

/// Stereo gain stage: velocity/volume, pan and controller modulation.
pub struct SIMDVoiceGainStereo<S: Simd> {
    left: f32,
    right: f32,
    dynamics: Option<Box<GainDynamics>>,
    _s: PhantomData<S>,
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
        Self {
            left: l,
            right: r,
            dynamics: modulation.map(|modulation| {
                Box::new(GainDynamics {
                    ramp_left: Ramp::new(l),
                    ramp_right: Ramp::new(r),
                    ramp_frames,
                    amp,
                    pan,
                    modulation,
                })
            }),
            _s: PhantomData,
        }
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
        if let Some(d) = self.dynamics.as_mut() {
            let (l, r) = Self::compute(d.amp, d.pan, Some(&d.modulation), &control.controllers);
            d.ramp_left.set(l, d.ramp_frames);
            d.ramp_right.set(r, d.ramp_frames);
        }
    }
}

impl<S: Simd> SIMDVoiceGenerator<S, SIMDSampleStereo<S>> for SIMDVoiceGainStereo<S> {
    #[inline(always)]
    fn next_sample(&mut self) -> SIMDSampleStereo<S> {
        if let Some(d) = self.dynamics.as_mut() {
            if d.ramp_left.remaining > 0 {
                self.left = d.ramp_left.next();
                self.right = d.ramp_right.next();
            }
        }
        simd_block!(S, {
            SIMDSampleStereo(S::Vf32::set1(self.left), S::Vf32::set1(self.right))
        })
    }
}

/// Mono gain stage: velocity/volume and controller modulation.
pub struct SIMDVoiceGainMono<S: Simd> {
    value: f32,
    dynamics: Option<Box<GainDynamics>>,
    _s: PhantomData<S>,
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
        Self {
            value,
            dynamics: modulation.map(|modulation| {
                Box::new(GainDynamics {
                    ramp_left: Ramp::new(value),
                    ramp_right: Ramp::new(value),
                    ramp_frames,
                    amp,
                    pan: 0.5,
                    modulation,
                })
            }),
            _s: PhantomData,
        }
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
        if let Some(d) = self.dynamics.as_mut() {
            let value = Self::compute(d.amp, Some(&d.modulation), &control.controllers);
            d.ramp_left.set(value, d.ramp_frames);
        }
    }
}

impl<S: Simd> SIMDVoiceGenerator<S, SIMDSampleMono<S>> for SIMDVoiceGainMono<S> {
    #[inline(always)]
    fn next_sample(&mut self) -> SIMDSampleMono<S> {
        if let Some(d) = self.dynamics.as_mut() {
            if d.ramp_left.remaining > 0 {
                self.value = d.ramp_left.next();
            }
        }
        simd_block!(S, SIMDSampleMono(S::Vf32::set1(self.value)))
    }
}

/// Pitch (playback speed) stage: base speed, channel pitch (bend/tuning/vibrato),
/// controller-driven pitch modulation and portamento.
pub struct SIMDVoicePitch<S: Simd> {
    value: f32,
    base: f32,
    channel_mult: f32,
    mod_mult: f32,
    modulation: Option<Arc<RegionModulation>>,
    /// Portamento in progress, kept out of line to keep voices small
    glide: Option<Box<Glide>>,
    _s: PhantomData<S>,
}

/// A pitch glide: the remaining offset in cents and its change per vector.
struct Glide {
    cents: f32,
    step: f32,
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
        let mut pitch = Self {
            value: base,
            base,
            channel_mult: control.voice_pitch_multiplier,
            mod_mult: 1.0,
            modulation,
            glide: (glide_cents != 0.0).then(|| {
                Box::new(Glide {
                    cents: glide_cents,
                    step: glide_step,
                })
            }),
            _s: PhantomData,
        };
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
        if let Some(glide) = &self.glide {
            value *= cents_factor(glide.cents);
        }
        self.value = value;
    }
}

impl<S: Simd> VoiceGeneratorBase for SIMDVoicePitch<S> {
    #[inline(always)]
    fn speed_hint(&self) -> f32 {
        self.value
    }

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
    const UNIFORM: bool = true;

    #[inline(always)]
    fn next_sample(&mut self) -> SIMDSampleMono<S> {
        if let Some(glide) = self.glide.as_mut() {
            if glide.cents > 0.0 {
                glide.cents = (glide.cents - glide.step).max(0.0);
            } else {
                glide.cents = (glide.cents + glide.step).min(0.0);
            }
            if glide.cents == 0.0 {
                self.glide = None;
            }
            self.refresh();
        }
        simd_block!(S, SIMDSampleMono(S::Vf32::set1(self.value)))
    }
}
