use crate::{
    modulation::{
        shape_value, EnvelopeTarget, ModController, ModCurve, ModDestination, ModSource,
        RegionModulation, RuntimeModulator,
    },
    sfz::AmpegEnvelopeParams,
};
use soundfont::raw::{
    default_modulators, ControllerPalette, GeneralPalette, GeneratorType, Modulator,
    ModulatorTransform, SourceDirection, SourcePolarity, SourceType,
};

use super::Sf2Region;

const DEFAULT_FILTER_CUTOFF_CENTS: f32 = 13_500.0;
const DEFAULT_VOL_ENV_TIMECENTS: f32 = -12_000.0;

#[derive(Clone, Debug, Default)]
pub struct Sf2NoteParams {
    pub volume: f32,
    pub pan: i16,
    pub cutoff: Option<f32>,
    pub resonance: f32,
    pub tune_cents: f32,
    pub ampeg_envelope: AmpegEnvelopeParams,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Sf2RawEnvelope {
    pub delay_tc: i32,
    pub attack_tc: i32,
    pub hold_tc: i32,
    pub decay_tc: i32,
    pub sustain_cb: i32,
    pub release_tc: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Sf2NoteModulation {
    attenuation_cb: f32,
    cutoff_cents: f32,
    resonance_db: f32,
    pan: f32,
    tune_cents: f32,
    delay_tc: f32,
    attack_tc: f32,
    hold_tc: f32,
    decay_tc: f32,
    sustain_cb: f32,
    release_tc: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sf2NoteModDestination {
    InitialAttenuation,
    InitialFilterFc,
    InitialFilterQ,
    Pan,
    DelayVolEnv,
    AttackVolEnv,
    HoldVolEnv,
    DecayVolEnv,
    SustainVolEnv,
    ReleaseVolEnv,
    FineTune,
    CoarseTune,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Sf2NoteModulator {
    src: Sf2NoteModSource,
    amount_src: Option<Sf2NoteModSource>,
    dest: Sf2NoteModDestination,
    amount: i16,
    transform: Sf2NoteTransform,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Sf2NoteModSource {
    controller: Sf2NoteController,
    direction: Sf2NoteDirection,
    polarity: Sf2NotePolarity,
    curve: Sf2NoteCurve,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sf2NoteController {
    Velocity,
    KeyNumber,
    Cc(u8),
    PitchWheel,
    ChannelPressure,
}

impl Sf2NoteController {
    fn runtime(&self) -> Option<ModController> {
        match *self {
            Sf2NoteController::Velocity | Sf2NoteController::KeyNumber => None,
            Sf2NoteController::Cc(cc) => Some(ModController::Cc(cc)),
            Sf2NoteController::PitchWheel => Some(ModController::PitchWheel),
            Sf2NoteController::ChannelPressure => Some(ModController::ChannelPressure),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sf2NoteDirection {
    Positive,
    Negative,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sf2NotePolarity {
    Unipolar,
    Bipolar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sf2NoteCurve {
    Linear,
    Concave,
    Convex,
    Switch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sf2NoteTransform {
    Linear,
    Absolute,
}

impl Sf2Region {
    pub fn note_params(&self, key: u8, velocity: u8) -> Sf2NoteParams {
        let modulation = self.note_modulation(key, velocity);
        let hold_keytrack = keynum_to_timecents(self.keynum_to_vol_env_hold, key);
        let decay_keytrack = keynum_to_timecents(self.keynum_to_vol_env_decay, key);
        let cutoff = self
            .cutoff_cents
            .map(|cents| cents as f32 + modulation.cutoff_cents);
        let cutoff = if cutoff.is_some() || modulation.cutoff_cents != 0.0 {
            Some(raw_cutoff_to_hz(
                cutoff.unwrap_or(DEFAULT_FILTER_CUTOFF_CENTS),
            ))
        } else {
            None
        };

        let raw_env = self.raw_envelope;
        let ampeg_envelope = AmpegEnvelopeParams {
            ampeg_start: 0.0,
            ampeg_delay: timecents_to_seconds(raw_env.delay_tc as f32 + modulation.delay_tc),
            ampeg_attack: timecents_to_seconds(raw_env.attack_tc as f32 + modulation.attack_tc),
            ampeg_hold: timecents_to_seconds(
                raw_env.hold_tc as f32 + hold_keytrack + modulation.hold_tc,
            ),
            ampeg_decay: timecents_to_seconds(
                raw_env.decay_tc as f32 + decay_keytrack + modulation.decay_tc,
            ),
            ampeg_sustain: sustain_cb_to_percent(raw_env.sustain_cb as f32 + modulation.sustain_cb),
            ampeg_release: timecents_to_seconds(raw_env.release_tc as f32 + modulation.release_tc),
            ampeg_vel2release: 0.0,
        };

        Sf2NoteParams {
            volume: self.volume * centibels_to_amp(modulation.attenuation_cb),
            pan: ((self.pan as f32 + modulation.pan).round() as i16).clamp(-500, 500),
            cutoff,
            resonance: self.resonance + modulation.resonance_db,
            tune_cents: modulation.tune_cents,
            ampeg_envelope,
        }
    }

    fn note_modulation(&self, key: u8, velocity: u8) -> Sf2NoteModulation {
        let mut out = Sf2NoteModulation::default();

        for modulator in self.note_modulators.iter().filter(|m| m.is_note_only()) {
            let amount = modulator.evaluate(key, velocity);
            if amount == 0.0 {
                continue;
            }

            match modulator.dest {
                Sf2NoteModDestination::InitialAttenuation => out.attenuation_cb += amount,
                Sf2NoteModDestination::InitialFilterFc => out.cutoff_cents += amount,
                Sf2NoteModDestination::InitialFilterQ => out.resonance_db += amount / 10.0,
                Sf2NoteModDestination::Pan => out.pan += amount,
                Sf2NoteModDestination::DelayVolEnv => out.delay_tc += amount,
                Sf2NoteModDestination::AttackVolEnv => out.attack_tc += amount,
                Sf2NoteModDestination::HoldVolEnv => out.hold_tc += amount,
                Sf2NoteModDestination::DecayVolEnv => out.decay_tc += amount,
                Sf2NoteModDestination::SustainVolEnv => out.sustain_cb += amount,
                Sf2NoteModDestination::ReleaseVolEnv => out.release_tc += amount,
                Sf2NoteModDestination::FineTune => out.tune_cents += amount,
                Sf2NoteModDestination::CoarseTune => out.tune_cents += amount * 100.0,
            }
        }

        out
    }

    /// Returns the controller-driven modulators of this region for a note,
    /// with any key/velocity source folded into the amount.
    pub fn runtime_modulation(&self, key: u8, velocity: u8) -> RegionModulation {
        let modulators = self
            .note_modulators
            .iter()
            .filter_map(|m| m.to_runtime(key, velocity))
            .collect();
        RegionModulation {
            modulators,
            amplitude_percent: 100.0,
            ..Default::default()
        }
    }
}

impl Sf2NoteModulator {
    pub(crate) fn is_note_only(&self) -> bool {
        self.src.controller.runtime().is_none()
            && self
                .amount_src
                .is_none_or(|src| src.controller.runtime().is_none())
    }

    fn to_runtime(self, key: u8, velocity: u8) -> Option<RuntimeModulator> {
        if self.is_note_only() {
            return None;
        }

        // Fold note-on sources into the amount so only live controllers remain.
        let mut amount = self.amount as f32;
        let (source, amount_source) = match (self.src.to_runtime(), self.amount_src) {
            (Some(src), None) => (src, None),
            (Some(src), Some(amt)) => match amt.to_runtime() {
                Some(amt) => (src, Some(amt)),
                None => {
                    amount *= amt.evaluate(key, velocity);
                    (src, None)
                }
            },
            (None, Some(amt)) => {
                amount *= self.src.evaluate(key, velocity);
                (amt.to_runtime()?, None)
            }
            (None, None) => return None,
        };

        let (destination, amount) = match self.dest {
            // Attenuation is in centibels, positive values make the voice quieter.
            Sf2NoteModDestination::InitialAttenuation => (ModDestination::VolumeDb, -amount / 10.0),
            Sf2NoteModDestination::InitialFilterFc => (ModDestination::CutoffCents, amount),
            Sf2NoteModDestination::InitialFilterQ => (ModDestination::ResonanceDb, amount / 10.0),
            // Pan is in 0.1% units, -500 is full left.
            Sf2NoteModDestination::Pan => (ModDestination::Pan, amount / 500.0),
            Sf2NoteModDestination::FineTune => (ModDestination::PitchCents, amount),
            Sf2NoteModDestination::CoarseTune => (ModDestination::PitchCents, amount * 100.0),
            Sf2NoteModDestination::DelayVolEnv => {
                (ModDestination::EnvelopeLog(EnvelopeTarget::Delay), amount)
            }
            Sf2NoteModDestination::AttackVolEnv => {
                (ModDestination::EnvelopeLog(EnvelopeTarget::Attack), amount)
            }
            Sf2NoteModDestination::HoldVolEnv => {
                (ModDestination::EnvelopeLog(EnvelopeTarget::Hold), amount)
            }
            Sf2NoteModDestination::DecayVolEnv => {
                (ModDestination::EnvelopeLog(EnvelopeTarget::Decay), amount)
            }
            Sf2NoteModDestination::SustainVolEnv => {
                (ModDestination::EnvelopeLog(EnvelopeTarget::Sustain), amount)
            }
            Sf2NoteModDestination::ReleaseVolEnv => {
                (ModDestination::EnvelopeLog(EnvelopeTarget::Release), amount)
            }
        };

        Some(RuntimeModulator {
            source,
            amount_source,
            amount,
            absolute: self.transform == Sf2NoteTransform::Absolute,
            destination,
        })
    }

    pub(crate) fn parse_zone(modulator: &Modulator) -> Option<Self> {
        // The SF2 default controller modulators (CC7, CC10, CC11, pitch wheel, ...) are
        // implemented once per channel. A zone redefining one of them replaces the
        // default with the same behaviour, so it is skipped here instead of being
        // applied twice.
        if is_default_controller_modulator(modulator) {
            return None;
        }
        Self::parse(modulator)
    }

    pub(crate) fn destination(&self) -> Sf2NoteModDestination {
        self.dest
    }

    pub(crate) fn same_identity(&self, other: &Self) -> bool {
        self.dest == other.dest
            && self.src == other.src
            && self.amount_src == other.amount_src
            && self.transform == other.transform
    }

    pub(crate) fn suppresses_default(&self, default: &Self) -> bool {
        if self.same_identity(default) {
            return true;
        }

        matches!(
            (
                self.dest,
                self.src.controller,
                default.dest,
                default.src.controller,
            ),
            (
                Sf2NoteModDestination::InitialAttenuation,
                Sf2NoteController::Velocity,
                Sf2NoteModDestination::InitialAttenuation,
                Sf2NoteController::Velocity,
            ) | (
                Sf2NoteModDestination::InitialFilterFc,
                Sf2NoteController::Velocity,
                Sf2NoteModDestination::InitialFilterFc,
                Sf2NoteController::Velocity,
            )
        )
    }

    pub(crate) fn parse(modulator: &Modulator) -> Option<Self> {
        Some(Self {
            src: Sf2NoteModSource::parse_primary(modulator)?,
            amount_src: Sf2NoteModSource::parse_secondary(modulator)?,
            dest: match modulator.dest {
                GeneratorType::InitialAttenuation => Sf2NoteModDestination::InitialAttenuation,
                GeneratorType::InitialFilterFc => Sf2NoteModDestination::InitialFilterFc,
                GeneratorType::InitialFilterQ => Sf2NoteModDestination::InitialFilterQ,
                GeneratorType::Pan => Sf2NoteModDestination::Pan,
                GeneratorType::DelayVolEnv => Sf2NoteModDestination::DelayVolEnv,
                GeneratorType::AttackVolEnv => Sf2NoteModDestination::AttackVolEnv,
                GeneratorType::HoldVolEnv => Sf2NoteModDestination::HoldVolEnv,
                GeneratorType::DecayVolEnv => Sf2NoteModDestination::DecayVolEnv,
                GeneratorType::SustainVolEnv => Sf2NoteModDestination::SustainVolEnv,
                GeneratorType::ReleaseVolEnv => Sf2NoteModDestination::ReleaseVolEnv,
                GeneratorType::FineTune => Sf2NoteModDestination::FineTune,
                GeneratorType::CoarseTune => Sf2NoteModDestination::CoarseTune,
                _ => return None,
            },
            amount: modulator.amount,
            transform: match modulator.transform {
                ModulatorTransform::Linear => Sf2NoteTransform::Linear,
                ModulatorTransform::Absolute => Sf2NoteTransform::Absolute,
            },
        })
    }

    fn evaluate(&self, key: u8, velocity: u8) -> f32 {
        let mut value = self.amount as f32 * self.src.evaluate(key, velocity);

        if let Some(amount_src) = self.amount_src {
            value *= amount_src.evaluate(key, velocity);
        }

        match self.transform {
            Sf2NoteTransform::Linear => value,
            Sf2NoteTransform::Absolute => value.abs(),
        }
    }
}

impl Sf2NoteModSource {
    fn parse_primary(modulator: &Modulator) -> Option<Self> {
        Self::parse(modulator.src)
    }

    fn parse_secondary(modulator: &Modulator) -> Option<Option<Self>> {
        match modulator.amt_src.controller_palette {
            ControllerPalette::General(GeneralPalette::NoController) => Some(None),
            _ => Self::parse(modulator.amt_src).map(Some),
        }
    }

    fn parse(source: soundfont::raw::ModulatorSource) -> Option<Self> {
        Some(Self {
            controller: match source.controller_palette {
                ControllerPalette::General(GeneralPalette::NoteOnVelocity) => {
                    Sf2NoteController::Velocity
                }
                ControllerPalette::General(GeneralPalette::NoteOnKeyNumber) => {
                    Sf2NoteController::KeyNumber
                }
                ControllerPalette::General(GeneralPalette::PitchWheel) => {
                    Sf2NoteController::PitchWheel
                }
                ControllerPalette::General(GeneralPalette::ChannelPressure) => {
                    Sf2NoteController::ChannelPressure
                }
                // CC 0-31 LSBs pair with their MSB, and 96-127 are channel mode / RPN
                // controllers which the SF2 spec does not allow as sources.
                ControllerPalette::Midi(cc) if cc < 120 => Sf2NoteController::Cc(cc),
                _ => return None,
            },
            direction: match source.direction {
                SourceDirection::Positive => Sf2NoteDirection::Positive,
                SourceDirection::Negative => Sf2NoteDirection::Negative,
            },
            polarity: match source.polarity {
                SourcePolarity::Unipolar => Sf2NotePolarity::Unipolar,
                SourcePolarity::Bipolar => Sf2NotePolarity::Bipolar,
            },
            curve: match source.ty {
                SourceType::Linear => Sf2NoteCurve::Linear,
                SourceType::Concave => Sf2NoteCurve::Concave,
                SourceType::Convex => Sf2NoteCurve::Convex,
                SourceType::Switch => Sf2NoteCurve::Switch,
                _ => return None,
            },
        })
    }

    fn evaluate(&self, key: u8, velocity: u8) -> f32 {
        let value = match self.controller {
            Sf2NoteController::Velocity => velocity as f32,
            Sf2NoteController::KeyNumber => key as f32,
            _ => return 1.0,
        };
        self.shape(value / 127.0)
    }

    fn shape(&self, normalized: f32) -> f32 {
        shape_value(
            self.curve.into(),
            self.polarity == Sf2NotePolarity::Bipolar,
            self.direction == Sf2NoteDirection::Negative,
            normalized,
        )
    }

    fn to_runtime(self) -> Option<ModSource> {
        Some(ModSource {
            controller: self.controller.runtime()?,
            curve: self.curve.into(),
            bipolar: self.polarity == Sf2NotePolarity::Bipolar,
            negative: self.direction == Sf2NoteDirection::Negative,
        })
    }
}

impl From<Sf2NoteCurve> for ModCurve {
    fn from(value: Sf2NoteCurve) -> Self {
        match value {
            Sf2NoteCurve::Linear => ModCurve::Linear,
            Sf2NoteCurve::Concave => ModCurve::Concave,
            Sf2NoteCurve::Convex => ModCurve::Convex,
            Sf2NoteCurve::Switch => ModCurve::Switch,
        }
    }
}

fn is_default_controller_modulator(modulator: &Modulator) -> bool {
    use default_modulators::*;
    [
        &DEFAULT_AT2VIBLFO_MOD,
        &DEFAULT_MOD2VIBLFO_MOD,
        &DEFAULT_ATT_MOD,
        &DEFAULT_PAN_MOD,
        &DEFAULT_EXPR_MOD,
        &DEFAULT_REVERB_MOD,
        &DEFAULT_CHORUS_MOD,
    ]
    .iter()
    .any(|default| {
        default.src == modulator.src
            && default.amt_src == modulator.amt_src
            && default.dest == modulator.dest
            && default.transform == modulator.transform
    })
}

#[allow(clippy::excessive_precision)]
const CONCAVE_TABLE: [f32; 128] = [
    0.000000000,
    0.001430490,
    0.002872378,
    0.004325848,
    0.005791087,
    0.007268288,
    0.008757646,
    0.010259365,
    0.011773650,
    0.013300714,
    0.014840775,
    0.016394055,
    0.017960784,
    0.019541196,
    0.021135532,
    0.022744041,
    0.024366976,
    0.026004598,
    0.027657176,
    0.029324986,
    0.031008310,
    0.032707440,
    0.034422676,
    0.036154326,
    0.037902707,
    0.039668145,
    0.041450978,
    0.043251550,
    0.045070219,
    0.046907352,
    0.048763328,
    0.050638537,
    0.052533382,
    0.054448278,
    0.056383655,
    0.058339956,
    0.060317637,
    0.062317171,
    0.064339048,
    0.066383770,
    0.068451862,
    0.070543862,
    0.072660331,
    0.074801848,
    0.076969012,
    0.079162445,
    0.081382793,
    0.083630722,
    0.085906929,
    0.088212133,
    0.090547082,
    0.092912554,
    0.095309357,
    0.097738334,
    0.100200359,
    0.102696344,
    0.105227238,
    0.107794034,
    0.110397763,
    0.113039503,
    0.115720383,
    0.118441577,
    0.121204318,
    0.124009895,
    0.126859655,
    0.129755013,
    0.132697452,
    0.135688529,
    0.138729879,
    0.141823220,
    0.144970361,
    0.148173206,
    0.151433763,
    0.154754150,
    0.158136605,
    0.161583491,
    0.165097310,
    0.168680715,
    0.172336517,
    0.176067701,
    0.179877443,
    0.183769121,
    0.187746336,
    0.191812935,
    0.195973027,
    0.200231013,
    0.204591610,
    0.209059887,
    0.213641297,
    0.218341718,
    0.223167499,
    0.228125508,
    0.233223199,
    0.238468668,
    0.243870742,
    0.249439059,
    0.255184178,
    0.261117694,
    0.267252385,
    0.273602371,
    0.280183315,
    0.287012655,
    0.294109880,
    0.301496866,
    0.309198285,
    0.317242100,
    0.325660178,
    0.334489052,
    0.343770883,
    0.353554673,
    0.363897833,
    0.374868224,
    0.386546859,
    0.399031536,
    0.412441820,
    0.426926031,
    0.442671265,
    0.459918217,
    0.478983838,
    0.500297389,
    0.524460700,
    0.552355196,
    0.585347382,
    0.625726554,
    0.677784361,
    0.751155719,
    0.876584884,
    1.000000000,
];

#[allow(clippy::excessive_precision)]
const CONVEX_TABLE: [f32; 128] = [
    0.000000000,
    0.123415116,
    0.248844281,
    0.322215639,
    0.374273446,
    0.414652618,
    0.447644804,
    0.475539300,
    0.499702611,
    0.521016162,
    0.540081783,
    0.557328735,
    0.573073969,
    0.587558180,
    0.600968464,
    0.613453141,
    0.625131776,
    0.636102167,
    0.646445327,
    0.656229117,
    0.665510948,
    0.674339822,
    0.682757900,
    0.690801715,
    0.698503134,
    0.705890120,
    0.712987345,
    0.719816685,
    0.726397629,
    0.732747615,
    0.738882306,
    0.744815822,
    0.750560941,
    0.756129258,
    0.761531332,
    0.766776801,
    0.771874492,
    0.776832501,
    0.781658282,
    0.786358703,
    0.790940113,
    0.795408390,
    0.799768987,
    0.804026973,
    0.808187065,
    0.812253664,
    0.816230879,
    0.820122557,
    0.823932299,
    0.827663483,
    0.831319285,
    0.834902690,
    0.838416509,
    0.841863395,
    0.845245850,
    0.848566237,
    0.851826794,
    0.855029639,
    0.858176780,
    0.861270121,
    0.864311471,
    0.867302548,
    0.870244987,
    0.873140345,
    0.875990105,
    0.878795682,
    0.881558423,
    0.884279617,
    0.886960497,
    0.889602237,
    0.892205966,
    0.894772762,
    0.897303656,
    0.899799641,
    0.902261666,
    0.904690643,
    0.907087446,
    0.909452918,
    0.911787867,
    0.914093071,
    0.916369278,
    0.918617207,
    0.920837555,
    0.923030988,
    0.925198152,
    0.927339669,
    0.929456138,
    0.931548138,
    0.933616230,
    0.935660952,
    0.937682829,
    0.939682363,
    0.941660044,
    0.943616345,
    0.945551722,
    0.947466618,
    0.949361463,
    0.951236672,
    0.953092648,
    0.954929781,
    0.956748450,
    0.958549022,
    0.960331855,
    0.962097293,
    0.963845674,
    0.965577324,
    0.967292560,
    0.968991690,
    0.970675014,
    0.972342824,
    0.973995402,
    0.975633024,
    0.977255959,
    0.978864468,
    0.980458804,
    0.982039216,
    0.983605945,
    0.985159225,
    0.986699286,
    0.988226350,
    0.989740635,
    0.991242354,
    0.992731712,
    0.994208913,
    0.995674152,
    0.997127622,
    0.998569510,
    1.000000000,
];

fn lookup_controller_curve(table: &[f32; 128], normalized: f32) -> f32 {
    let scaled = normalized.clamp(0.0, 1.0) * 127.0;
    let index = scaled.floor() as usize;
    let next = (index + 1).min(127);
    let frac = scaled - index as f32;
    table[index] + (table[next] - table[index]) * frac
}

pub(crate) fn concave_lookup(normalized: f32) -> f32 {
    lookup_controller_curve(&CONCAVE_TABLE, normalized)
}

pub(crate) fn convex_lookup(normalized: f32) -> f32 {
    lookup_controller_curve(&CONVEX_TABLE, normalized)
}

fn keynum_to_timecents(amount: i16, key: u8) -> f32 {
    (60.0 - key as f32) * amount as f32
}

fn raw_cutoff_to_hz(cents: f32) -> f32 {
    2f32.powf(cents.clamp(1500.0, 13_500.0) / 1200.0) * 8.176
}

fn timecents_to_seconds(timecents: f32) -> f32 {
    if timecents <= -32_768.0 {
        0.0
    } else {
        2f32.powf(timecents.clamp(-12_000.0, 8_000.0) / 1200.0)
    }
}

fn sustain_cb_to_percent(cb: f32) -> f32 {
    10f32.powf(-cb.max(0.0) / 200.0) * 100.0
}

fn centibels_to_amp(cb: f32) -> f32 {
    10f32.powf(-cb.max(0.0) / 200.0)
}

pub(crate) fn default_raw_envelope() -> Sf2RawEnvelope {
    Sf2RawEnvelope {
        delay_tc: DEFAULT_VOL_ENV_TIMECENTS as i32,
        attack_tc: DEFAULT_VOL_ENV_TIMECENTS as i32,
        hold_tc: DEFAULT_VOL_ENV_TIMECENTS as i32,
        decay_tc: DEFAULT_VOL_ENV_TIMECENTS as i32,
        sustain_cb: 0,
        release_tc: DEFAULT_VOL_ENV_TIMECENTS as i32,
    }
}

pub(crate) fn default_note_modulators() -> [Sf2NoteModulator; 2] {
    [
        Sf2NoteModulator::default_velocity_to_attenuation(),
        Sf2NoteModulator::default_velocity_to_filter_cutoff(),
    ]
}

impl Sf2NoteModulator {
    fn default_velocity_to_attenuation() -> Self {
        Self::parse(&default_modulators::DEFAULT_VEL2ATT_MOD).unwrap()
    }

    fn default_velocity_to_filter_cutoff() -> Self {
        Self {
            src: Sf2NoteModSource {
                controller: Sf2NoteController::Velocity,
                direction: Sf2NoteDirection::Negative,
                polarity: Sf2NotePolarity::Unipolar,
                curve: Sf2NoteCurve::Linear,
            },
            amount_src: Some(Sf2NoteModSource {
                controller: Sf2NoteController::Velocity,
                direction: Sf2NoteDirection::Positive,
                polarity: Sf2NotePolarity::Unipolar,
                curve: Sf2NoteCurve::Switch,
            }),
            dest: Sf2NoteModDestination::InitialFilterFc,
            amount: -2400,
            transform: Sf2NoteTransform::Linear,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soundfont::raw::{
        ControllerPalette, GeneralPalette, GeneratorType, ModulatorSource, SourceDirection,
        SourcePolarity, SourceType,
    };

    fn source(
        palette: ControllerPalette,
        direction: SourceDirection,
        polarity: SourcePolarity,
        ty: SourceType,
    ) -> soundfont::raw::ModulatorSource {
        ModulatorSource {
            index: 0,
            controller_palette: palette,
            direction,
            polarity,
            ty,
        }
    }

    #[test]
    fn parses_velocity_attack_modulator() {
        let modulator = Modulator {
            src: source(
                ControllerPalette::General(GeneralPalette::NoteOnVelocity),
                SourceDirection::Positive,
                SourcePolarity::Unipolar,
                SourceType::Linear,
            ),
            dest: GeneratorType::AttackVolEnv,
            amount: 2400,
            amt_src: source(
                ControllerPalette::General(GeneralPalette::NoController),
                SourceDirection::Positive,
                SourcePolarity::Unipolar,
                SourceType::Linear,
            ),
            transform: ModulatorTransform::Linear,
        };

        let parsed = Sf2NoteModulator::parse(&modulator).unwrap();
        let value = parsed.evaluate(0, 127);
        assert!((value - 2400.0).abs() < 0.001);
    }

    #[test]
    fn parses_switch_secondary_source() {
        let modulator = Modulator {
            src: source(
                ControllerPalette::General(GeneralPalette::NoteOnVelocity),
                SourceDirection::Negative,
                SourcePolarity::Unipolar,
                SourceType::Linear,
            ),
            dest: GeneratorType::InitialFilterFc,
            amount: -2400,
            amt_src: source(
                ControllerPalette::General(GeneralPalette::NoteOnVelocity),
                SourceDirection::Negative,
                SourcePolarity::Unipolar,
                SourceType::Switch,
            ),
            transform: ModulatorTransform::Linear,
        };

        let parsed = Sf2NoteModulator::parse(&modulator).unwrap();
        assert_eq!(parsed.evaluate(60, 100), 0.0);
        assert!(parsed.evaluate(60, 40) < 0.0);
    }

    #[test]
    fn concave_curve_matches_reference_table() {
        let modulator = Modulator {
            src: source(
                ControllerPalette::General(GeneralPalette::NoteOnVelocity),
                SourceDirection::Negative,
                SourcePolarity::Unipolar,
                SourceType::Concave,
            ),
            dest: GeneratorType::InitialAttenuation,
            amount: 960,
            amt_src: source(
                ControllerPalette::General(GeneralPalette::NoController),
                SourceDirection::Positive,
                SourcePolarity::Unipolar,
                SourceType::Linear,
            ),
            transform: ModulatorTransform::Linear,
        };

        let parsed = Sf2NoteModulator::parse(&modulator).unwrap();
        let value = parsed.evaluate(0, 64);
        let expected = 960.0 * CONCAVE_TABLE[63];
        assert!((value - expected).abs() < 0.001);
    }

    #[test]
    fn default_velocity_filter_is_suppressed_by_explicit_velocity_filter_mod() {
        let default = Sf2NoteModulator::default_velocity_to_filter_cutoff();
        let explicit = Sf2NoteModulator::parse(&Modulator {
            src: source(
                ControllerPalette::General(GeneralPalette::NoteOnVelocity),
                SourceDirection::Negative,
                SourcePolarity::Unipolar,
                SourceType::Linear,
            ),
            dest: GeneratorType::InitialFilterFc,
            amount: 0,
            amt_src: source(
                ControllerPalette::General(GeneralPalette::NoteOnVelocity),
                SourceDirection::Positive,
                SourcePolarity::Unipolar,
                SourceType::Switch,
            ),
            transform: ModulatorTransform::Linear,
        })
        .unwrap();

        assert!(explicit.suppresses_default(&default));
    }

    #[test]
    fn cc_modulator_becomes_runtime_with_velocity_folded_in() {
        let modulator = Modulator {
            src: source(
                ControllerPalette::Midi(2),
                SourceDirection::Positive,
                SourcePolarity::Unipolar,
                SourceType::Linear,
            ),
            dest: GeneratorType::InitialAttenuation,
            amount: 200,
            amt_src: source(
                ControllerPalette::General(GeneralPalette::NoteOnVelocity),
                SourceDirection::Positive,
                SourcePolarity::Unipolar,
                SourceType::Linear,
            ),
            transform: ModulatorTransform::Linear,
        };

        let parsed = Sf2NoteModulator::parse_zone(&modulator).unwrap();
        assert!(!parsed.is_note_only());
        // Runtime modulators must not leak into the baked note-on values
        assert_eq!(parsed.evaluate(60, 127), 200.0);

        let runtime = parsed.to_runtime(60, 127).unwrap();
        assert_eq!(runtime.source.controller, ModController::Cc(2));
        assert!(runtime.amount_source.is_none());
        assert_eq!(runtime.destination, ModDestination::VolumeDb);
        // 200cB of attenuation at full velocity -> -20dB
        assert!((runtime.amount + 20.0).abs() < 1e-4);

        let half = parsed.to_runtime(60, 0).unwrap();
        assert_eq!(half.amount, 0.0);
    }

    #[test]
    fn default_controller_modulators_are_left_to_the_channel() {
        assert!(Sf2NoteModulator::parse_zone(&default_modulators::DEFAULT_ATT_MOD).is_none());
        assert!(Sf2NoteModulator::parse_zone(&default_modulators::DEFAULT_PAN_MOD).is_none());
        assert!(Sf2NoteModulator::parse_zone(&default_modulators::DEFAULT_EXPR_MOD).is_none());
        assert!(Sf2NoteModulator::parse_zone(&default_modulators::DEFAULT_VEL2ATT_MOD).is_some());
    }

    #[test]
    fn keynum_to_hold_tracks_around_middle_c() {
        assert_eq!(keynum_to_timecents(50, 60), 0.0);
        assert_eq!(keynum_to_timecents(50, 36), 1200.0);
        assert_eq!(keynum_to_timecents(50, 84), -1200.0);
    }
}
