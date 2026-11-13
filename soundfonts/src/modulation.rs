//! Runtime (controller-driven) modulation data shared by the SFZ and SF2 loaders.
//!
//! Both soundfont formats are lowered into the same small description so the
//! synthesizer can evaluate controller-driven modulation with a single, cheap
//! code path. Everything that only depends on the note-on key and velocity is
//! still baked at load time; only the parts that depend on live MIDI
//! controllers end up here.

use crate::sf2::{concave_lookup, convex_lookup};

/// A live MIDI controller that can drive a modulator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModController {
    /// MIDI continuous controller (0-127).
    Cc(u8),
    /// Pitch wheel position.
    PitchWheel,
    /// Channel pressure (aftertouch).
    ChannelPressure,
}

/// Mapping curve applied to a normalized controller value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ModCurve {
    #[default]
    Linear,
    Concave,
    Convex,
    Switch,
}

/// A modulation source: a controller plus the shape applied to its value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModSource {
    pub controller: ModController,
    pub curve: ModCurve,
    /// Maps the controller to -1..1 instead of 0..1.
    pub bipolar: bool,
    /// Reverses the controller direction.
    pub negative: bool,
}

impl ModSource {
    /// A linear, unipolar, positive source, as used by SFZ `*_onccN` opcodes.
    pub const fn linear(controller: ModController) -> Self {
        Self {
            controller,
            curve: ModCurve::Linear,
            bipolar: false,
            negative: false,
        }
    }

    /// Applies the curve to a controller value normalized to 0..1.
    #[inline]
    pub fn shape(&self, normalized: f32) -> f32 {
        shape_value(self.curve, self.bipolar, self.negative, normalized)
    }
}

/// Envelope stage targeted by a note-on envelope modulator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvelopeTarget {
    Start,
    Delay,
    Attack,
    Hold,
    Decay,
    Sustain,
    Release,
}

/// Destination of a controller-driven modulator, including its unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ModDestination {
    /// Gain offset in decibels.
    VolumeDb,
    /// Amplitude offset in percent, added to the region's base amplitude (SFZ `amplitude`).
    AmplitudePercent,
    /// Pan offset, where -1 is a full left shift and 1 a full right shift.
    Pan,
    /// Pitch offset in cents.
    PitchCents,
    /// Filter cutoff offset in cents.
    CutoffCents,
    /// Filter resonance offset in decibels.
    ResonanceDb,
    /// SFZ style envelope offset, applied at note-on. Times are in seconds,
    /// start and sustain in percent.
    EnvelopeLinear(EnvelopeTarget),
    /// SF2 style envelope offset, applied at note-on. Times are in timecents,
    /// sustain in centibels of attenuation.
    EnvelopeLog(EnvelopeTarget),
}

impl ModDestination {
    /// Whether the destination is only evaluated once when the voice starts.
    pub fn is_note_on_only(&self) -> bool {
        matches!(
            self,
            ModDestination::EnvelopeLinear(_) | ModDestination::EnvelopeLog(_)
        )
    }
}

/// A single controller-driven modulator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RuntimeModulator {
    pub source: ModSource,
    pub amount_source: Option<ModSource>,
    /// Amount in the destination's unit, already scaled by any note-on
    /// (key/velocity) factors.
    pub amount: f32,
    /// Take the absolute value of the result (SF2 absolute transform).
    pub absolute: bool,
    pub destination: ModDestination,
}

/// SFZ crossfade curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CrossfadeCurve {
    /// Constant power crossfade.
    #[default]
    Power,
    /// Linear gain crossfade.
    Gain,
}

/// A controller-driven crossfade (`xfin_loccN`/`xfin_hiccN`, `xfout_loccN`/`xfout_hiccN`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CcCrossfade {
    pub cc: u8,
    pub lo: u8,
    pub hi: u8,
    /// `true` for a fade in (xfin), `false` for a fade out (xfout).
    pub fade_in: bool,
}

impl CcCrossfade {
    /// Returns the gain factor of this crossfade for the given controller value.
    #[inline]
    pub fn gain(&self, value: u8, curve: CrossfadeCurve) -> f32 {
        let pos = if value <= self.lo {
            0.0
        } else if value >= self.hi {
            1.0
        } else {
            (value - self.lo) as f32 / (self.hi - self.lo) as f32
        };
        let pos = if self.fade_in { pos } else { 1.0 - pos };
        match curve {
            CrossfadeCurve::Gain => pos,
            CrossfadeCurve::Power => (pos * std::f32::consts::FRAC_PI_2).sin(),
        }
    }
}

/// An inclusive controller value range (`loccN`/`hiccN`, `on_loccN`/`on_hiccN`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CcRange {
    pub cc: u8,
    pub lo: u8,
    pub hi: u8,
}

impl CcRange {
    #[inline]
    pub fn contains(&self, value: u8) -> bool {
        value >= self.lo && value <= self.hi
    }
}

/// Set of MIDI controllers (CC 0-127 plus pitch wheel and channel pressure).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ControllerMask {
    bits: [u64; 3],
}

impl ControllerMask {
    pub const EMPTY: Self = Self { bits: [0; 3] };
    const PITCH_WHEEL: usize = 128;
    const CHANNEL_PRESSURE: usize = 129;

    fn index(controller: ModController) -> usize {
        match controller {
            ModController::Cc(cc) => (cc & 0x7F) as usize,
            ModController::PitchWheel => Self::PITCH_WHEEL,
            ModController::ChannelPressure => Self::CHANNEL_PRESSURE,
        }
    }

    pub fn insert(&mut self, controller: ModController) {
        let i = Self::index(controller);
        self.bits[i / 64] |= 1 << (i % 64);
    }

    #[inline]
    pub fn contains(&self, controller: ModController) -> bool {
        let i = Self::index(controller);
        self.bits[i / 64] & (1 << (i % 64)) != 0
    }

    pub fn union(&self, other: &Self) -> Self {
        Self {
            bits: [
                self.bits[0] | other.bits[0],
                self.bits[1] | other.bits[1],
                self.bits[2] | other.bits[2],
            ],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.bits == [0; 3]
    }
}

/// All controller-driven behaviour of a single region.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RegionModulation {
    pub modulators: Vec<RuntimeModulator>,
    pub crossfades: Vec<CcCrossfade>,
    pub crossfade_curve: CrossfadeCurve,
    /// Base amplitude in percent. Only used when amplitude modulators exist,
    /// otherwise it is baked into the region volume.
    pub amplitude_percent: f32,
}

impl RegionModulation {
    pub fn is_empty(&self) -> bool {
        self.modulators.is_empty() && self.crossfades.is_empty()
    }

    /// Whether any modulator or crossfade needs re-evaluating while the voice plays.
    pub fn has_realtime(&self) -> bool {
        !self.crossfades.is_empty()
            || self
                .modulators
                .iter()
                .any(|m| !m.destination.is_note_on_only())
    }

    pub fn has_note_on_envelope(&self) -> bool {
        self.modulators
            .iter()
            .any(|m| m.destination.is_note_on_only())
    }

    pub fn has_destination(&self, f: impl Fn(&ModDestination) -> bool) -> bool {
        self.modulators.iter().any(|m| f(&m.destination))
    }

    /// Controllers whose changes affect voices of this region while they play.
    pub fn realtime_controllers(&self) -> ControllerMask {
        let mut mask = ControllerMask::EMPTY;
        for m in self
            .modulators
            .iter()
            .filter(|m| !m.destination.is_note_on_only())
        {
            mask.insert(m.source.controller);
            if let Some(src) = m.amount_source {
                mask.insert(src.controller);
            }
        }
        for xf in &self.crossfades {
            mask.insert(ModController::Cc(xf.cc));
        }
        mask
    }
}

/// Applies a curve, polarity and direction to a value normalized to 0..1.
#[inline]
pub fn shape_value(curve: ModCurve, bipolar: bool, negative: bool, normalized: f32) -> f32 {
    let x = normalized.clamp(0.0, 1.0);
    if !bipolar {
        let x = if negative { 1.0 - x } else { x };
        match curve {
            ModCurve::Linear => x,
            ModCurve::Concave => concave_lookup(x),
            ModCurve::Convex => convex_lookup(x),
            ModCurve::Switch => {
                if x >= 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
        }
    } else {
        let value = match curve {
            ModCurve::Linear => -1.0 + 2.0 * x,
            ModCurve::Concave => bipolar_map(x, concave_lookup),
            ModCurve::Convex => bipolar_map(x, convex_lookup),
            ModCurve::Switch => {
                if x >= 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
        };
        if negative {
            -value
        } else {
            value
        }
    }
}

fn bipolar_map(normalized: f32, map: fn(f32) -> f32) -> f32 {
    if normalized > 0.5 {
        map((normalized - 0.5) * 2.0)
    } else {
        -map((0.5 - normalized) * 2.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossfade_gain_curves() {
        let xf = CcCrossfade {
            cc: 1,
            lo: 0,
            hi: 100,
            fade_in: true,
        };
        assert_eq!(xf.gain(0, CrossfadeCurve::Gain), 0.0);
        assert_eq!(xf.gain(50, CrossfadeCurve::Gain), 0.5);
        assert_eq!(xf.gain(127, CrossfadeCurve::Gain), 1.0);
        assert!((xf.gain(50, CrossfadeCurve::Power) - 0.70710677).abs() < 1e-5);

        let out = CcCrossfade {
            fade_in: false,
            ..xf
        };
        assert_eq!(out.gain(0, CrossfadeCurve::Gain), 1.0);
        assert_eq!(out.gain(127, CrossfadeCurve::Gain), 0.0);
    }

    #[test]
    fn controller_mask_tracks_controllers() {
        let mut mask = ControllerMask::EMPTY;
        assert!(mask.is_empty());
        mask.insert(ModController::Cc(1));
        mask.insert(ModController::Cc(127));
        mask.insert(ModController::PitchWheel);
        assert!(mask.contains(ModController::Cc(1)));
        assert!(mask.contains(ModController::Cc(127)));
        assert!(mask.contains(ModController::PitchWheel));
        assert!(!mask.contains(ModController::Cc(2)));
        assert!(!mask.contains(ModController::ChannelPressure));
    }

    #[test]
    fn shape_value_matches_sf2_polarities() {
        assert_eq!(shape_value(ModCurve::Linear, false, false, 1.0), 1.0);
        assert_eq!(shape_value(ModCurve::Linear, false, true, 1.0), 0.0);
        assert_eq!(shape_value(ModCurve::Linear, true, false, 0.0), -1.0);
        assert_eq!(shape_value(ModCurve::Linear, true, true, 0.0), 1.0);
        assert_eq!(shape_value(ModCurve::Switch, true, false, 0.2), -1.0);
    }
}
