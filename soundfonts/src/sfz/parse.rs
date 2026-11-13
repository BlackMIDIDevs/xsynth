use std::{
    collections::HashMap,
    ops::RangeInclusive,
    path::{Path, PathBuf},
};

use crate::{
    modulation::{CrossfadeCurve, EnvelopeTarget, ModController},
    FilterType, LoopMode,
};

use self::defines::apply_defines;

use super::grammar::{ErrorTolerantToken, Group, Opcode, Token, TokenKind};
use regex_bnf::{FileLocation, ParseError};
use thiserror::Error;

mod defines;
mod resolver;
use resolver::TokenResolver;

#[derive(Debug, Clone)]
pub enum SfzOpcode {
    Lovel(u8),
    Hivel(u8),
    Key(i8),
    Lokey(i8),
    Hikey(i8),
    PitchKeycenter(i8),
    Volume(i16),
    Pan(i8),
    Sample(String),
    LoopMode(LoopMode),
    LoopStart(u32),
    LoopEnd(u32),
    Offset(u32),
    Cutoff(f32),
    Resonance(f32),
    AmpKeycenter(i8),
    AmpKeytrack(f32),
    AmpVeltrack(f32),
    PanKeycenter(i8),
    PanKeytrack(f32),
    PanVeltrack(f32),
    FilVeltrack(i16),
    FilKeycenter(i8),
    FilKeytrack(i16),
    FilterType(FilterType),
    DefaultPath(String),
    Tune(i16),
    AmpegEnvelope(SfzAmpegEnvelope),
    Amplitude(f32),
    /// A controller-driven modulation opcode, such as `volume_onccN`.
    CcMod {
        target: SfzCcTarget,
        controller: ModController,
        value: f32,
    },
    /// `loccN` / `hiccN`
    CcCondition {
        cc: u8,
        high: bool,
        value: u8,
    },
    /// `on_loccN` / `on_hiccN` (also `start_loccN` / `start_hiccN`)
    CcTrigger {
        cc: u8,
        high: bool,
        value: u8,
    },
    /// `xfin_loccN`, `xfin_hiccN`, `xfout_loccN`, `xfout_hiccN`
    CcCrossfade {
        cc: u8,
        fade_in: bool,
        high: bool,
        value: u8,
    },
    XfCcCurve(CrossfadeCurve),
    /// `set_ccN` / `set_hdccN`, the initial value of a controller
    SetCc(u8, u8),
    /// `effect1`: reverb send in percent
    ReverbSend(f32),
    /// `effect2`: chorus send in percent
    ChorusSend(f32),
}

/// The parameter modulated by a controller-driven SFZ opcode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SfzCcTarget {
    Amplitude,
    Volume,
    Pan,
    Pitch,
    Cutoff,
    Resonance,
    Envelope(EnvelopeTarget),
}

#[derive(Debug, Clone)]
#[allow(clippy::enum_variant_names)]
pub enum SfzAmpegEnvelope {
    AmpegStart(f32),
    AmpegDelay(f32),
    AmpegAttack(f32),
    AmpegHold(f32),
    AmpegDecay(f32),
    AmpegSustain(f32),
    AmpegRelease(f32),
    AmpegVel2Release(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SfzGroupType {
    Region,
    Group,
    Master,
    Global,
    Control,
    Other,
}

#[derive(Debug, Clone)]
pub enum SfzToken {
    Group(SfzGroupType),
    Opcode(SfzOpcode),
}

#[derive(Debug, Clone)]
pub enum SfzTokenWithMeta {
    Group(SfzGroupType),
    Opcode(SfzOpcode),
    Import(String),
    Define(String, String),
}

/// Parameters of an error generated while validating an SFZ file.
#[derive(Error, Debug, Clone)]
pub struct SfzValidationError {
    pub pos: FileLocation,
    pub message: String,
}

impl SfzValidationError {
    #[allow(dead_code)]
    pub(super) fn new(pos: FileLocation, message: String) -> Self {
        Self { pos, message }
    }
}

impl std::fmt::Display for SfzValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {}", self.message, self.pos)
    }
}

/// Errors that can be generated when parsing an SFZ file.
#[derive(Error, Debug, Clone)]
pub enum SfzParseError {
    #[error("Failed to parse SFZ file: {0}")]
    GrammarError(#[from] ParseError),

    #[error("Failed to parse SFZ file: {0}")]
    ValidationError(#[from] SfzValidationError),

    #[error("Failed to read file: {0}")]
    FailedToReadFile(PathBuf),

    #[error("Cyclic include detected while reading: {0}")]
    IncludeCycle(PathBuf),
}

fn parse_key_number(val: &str) -> Option<i8> {
    match val.parse::<i8>().ok() {
        Some(val) => Some(val.clamp(-1, 127)),
        None => {
            let note: String = val
                .chars()
                .filter(|c| !(c.is_ascii_digit() || c == &'-'))
                .collect();
            let semitone: i16 = match note.to_lowercase().as_str() {
                "c" => 0,
                "c#" => 1,
                "db" => 1,
                "d" => 2,
                "d#" => 3,
                "eb" => 3,
                "e" => 4,
                "f" => 5,
                "f#" => 6,
                "gb" => 6,
                "g" => 7,
                "g#" => 8,
                "ab" => 8,
                "a" => 9,
                "a#" => 10,
                "bb" => 10,
                "b" => 11,
                _ => return None,
            };
            let octave: String = val
                .chars()
                .filter(|c| c.is_ascii_digit() || c == &'-')
                .collect();
            let octave: i16 = octave.parse().ok().unwrap_or(-10);
            if octave < -1 {
                None
            } else {
                let midi_note = 12 + semitone + octave * 12;
                Some(midi_note.clamp(-1, 127) as i8)
            }
        }
    }
}

fn parse_u8_in_range(val: &str, range: RangeInclusive<u8>) -> Option<u8> {
    val.parse()
        .ok()
        .map(|val: u8| val.clamp(*range.start(), *range.end()))
}

fn parse_i8_in_range(val: &str, range: RangeInclusive<i8>) -> Option<i8> {
    val.parse()
        .ok()
        .map(|val: i8| val.clamp(*range.start(), *range.end()))
}

fn parse_i16_in_range(val: &str, range: RangeInclusive<i16>) -> Option<i16> {
    val.parse()
        .ok()
        .map(|val: i16| val.clamp(*range.start(), *range.end()))
}

fn parse_u32_in_range(val: &str, range: RangeInclusive<u32>) -> Option<u32> {
    val.parse()
        .ok()
        .map(|val: u32| val.clamp(*range.start(), *range.end()))
}

fn parse_float_in_range(val: &str, range: RangeInclusive<f32>) -> Option<f32> {
    val.parse()
        .ok()
        .map(|val: f32| val.clamp(*range.start(), *range.end()))
}

fn parse_filter_kind(val: &str) -> Option<FilterType> {
    match val {
        "lpf_1p" => Some(FilterType::LowPassPole),
        "lpf_2p" => Some(FilterType::LowPass),
        "lpf_4p" => Some(FilterType::LowPass),
        "lpf_6p" => Some(FilterType::LowPass),
        "hpf_1p" => Some(FilterType::HighPass),
        "hpf_2p" => Some(FilterType::HighPass),
        "hpf_4p" => Some(FilterType::HighPass),
        "hpf_6p" => Some(FilterType::HighPass),
        "bpf_1p" => Some(FilterType::BandPass),
        "bpf_2p" => Some(FilterType::BandPass),
        _ => None,
    }
}

fn parse_loop_mode(val: &str) -> Option<LoopMode> {
    match val {
        "no_loop" => Some(LoopMode::NoLoop),
        "one_shot" => Some(LoopMode::OneShot),
        "loop_continuous" => Some(LoopMode::LoopContinuous),
        "loop_sustain" => Some(LoopMode::LoopSustain),
        _ => None,
    }
}

/// Splits an opcode name like `volume_oncc7` into `("volume_oncc", 7)`.
fn split_number_suffix(name: &str) -> Option<(&str, u16)> {
    let digits = name
        .bytes()
        .rev()
        .take_while(|b| b.is_ascii_digit())
        .count();
    if digits == 0 || digits == name.len() {
        return None;
    }
    let (head, num) = name.split_at(name.len() - digits);
    Some((head, num.parse().ok()?))
}

/// Maps an SFZ controller number to a controller, including the ARIA extended
/// controllers for pitch bend (128) and channel aftertouch (129).
fn sfz_controller(num: u16) -> Option<ModController> {
    match num {
        0..=127 => Some(ModController::Cc(num as u8)),
        128 => Some(ModController::PitchWheel),
        129 => Some(ModController::ChannelPressure),
        _ => None,
    }
}

fn cc_target(head: &str) -> Option<SfzCcTarget> {
    use EnvelopeTarget as E;
    use SfzCcTarget::*;
    Some(match head {
        "amplitude_oncc" | "amplitude_cc" => Amplitude,
        "volume_oncc" | "volume_cc" | "gain_oncc" | "gain_cc" => Volume,
        "pan_oncc" | "pan_cc" => Pan,
        "pitch_oncc" | "pitch_cc" | "tune_oncc" | "tune_cc" => Pitch,
        "cutoff_oncc" | "cutoff_cc" | "cutoff1_oncc" => Cutoff,
        "resonance_oncc" | "resonance_cc" | "resonance1_oncc" => Resonance,
        "ampeg_start_oncc" | "ampeg_startcc" => Envelope(E::Start),
        "ampeg_delay_oncc" | "ampeg_delaycc" => Envelope(E::Delay),
        "ampeg_attack_oncc" | "ampeg_attackcc" => Envelope(E::Attack),
        "ampeg_hold_oncc" | "ampeg_holdcc" => Envelope(E::Hold),
        "ampeg_decay_oncc" | "ampeg_decaycc" => Envelope(E::Decay),
        "ampeg_sustain_oncc" | "ampeg_sustaincc" => Envelope(E::Sustain),
        "ampeg_release_oncc" | "ampeg_releasecc" => Envelope(E::Release),
        _ => return None,
    })
}

fn parse_cc_opcode(name: &str, val: &str) -> Option<SfzOpcode> {
    use SfzOpcode::*;

    let (head, num) = split_number_suffix(name)?;

    if let Some(target) = cc_target(head) {
        let range = match target {
            SfzCcTarget::Amplitude => -100.0..=100.0,
            SfzCcTarget::Volume => -144.0..=48.0,
            SfzCcTarget::Pan => -200.0..=200.0,
            SfzCcTarget::Pitch | SfzCcTarget::Cutoff => -9600.0..=9600.0,
            SfzCcTarget::Resonance => -40.0..=40.0,
            SfzCcTarget::Envelope(_) => -100.0..=100.0,
        };
        return Some(CcMod {
            target,
            controller: sfz_controller(num)?,
            value: parse_float_in_range(val, range)?,
        });
    }

    // The remaining opcodes only accept regular MIDI controllers.
    let cc = u8::try_from(num).ok().filter(|cc| *cc < 128)?;
    let value = || parse_u8_in_range(val, 0..=127);
    Some(match head {
        "locc" => CcCondition {
            cc,
            high: false,
            value: value()?,
        },
        "hicc" => CcCondition {
            cc,
            high: true,
            value: value()?,
        },
        "on_locc" | "start_locc" => CcTrigger {
            cc,
            high: false,
            value: value()?,
        },
        "on_hicc" | "start_hicc" => CcTrigger {
            cc,
            high: true,
            value: value()?,
        },
        "xfin_locc" => CcCrossfade {
            cc,
            fade_in: true,
            high: false,
            value: value()?,
        },
        "xfin_hicc" => CcCrossfade {
            cc,
            fade_in: true,
            high: true,
            value: value()?,
        },
        "xfout_locc" => CcCrossfade {
            cc,
            fade_in: false,
            high: false,
            value: value()?,
        },
        "xfout_hicc" => CcCrossfade {
            cc,
            fade_in: false,
            high: true,
            value: value()?,
        },
        "set_cc" => SetCc(cc, value()?),
        "set_hdcc" => SetCc(
            cc,
            (parse_float_in_range(val, 0.0..=1.0)? * 127.0).round() as u8,
        ),
        _ => return None,
    })
}

fn parse_sfz_opcode(
    opcode: Opcode,
    defines: &HashMap<String, String>,
) -> Result<Option<SfzOpcode>, SfzValidationError> {
    let opcode_value = opcode.value.as_string();
    let name = apply_defines(opcode.name.name.text, defines);
    let val = apply_defines(&opcode_value, defines);

    use SfzAmpegEnvelope::*;
    use SfzOpcode::*;

    let val = val.as_ref();
    let name = name.as_ref();

    Ok(match name {
        "lokey" => parse_key_number(val).map(Lokey),
        "hikey" => parse_key_number(val).map(Hikey),
        "lovel" => parse_u8_in_range(val, 0..=128).map(Lovel),
        "hivel" => parse_u8_in_range(val, 0..=128).map(Hivel),
        "volume" => parse_i16_in_range(val, -144..=6).map(Volume),
        "pan" => parse_i8_in_range(val, -100..=100).map(Pan),
        "pitch_keycenter" => parse_key_number(val).map(PitchKeycenter),
        "key" => parse_key_number(val).map(Key),
        "cutoff" => parse_float_in_range(val, 1.0..=100000.0).map(Cutoff),
        "resonance" => parse_float_in_range(val, 0.0..=40.0).map(Resonance),
        "amp_keycenter" => parse_key_number(val).map(AmpKeycenter),
        "amp_keytrack" => parse_float_in_range(val, -96.0..=12.0).map(AmpKeytrack),
        "amp_veltrack" => parse_float_in_range(val, -100.0..=100.0).map(AmpVeltrack),
        "pan_keycenter" => parse_key_number(val).map(PanKeycenter),
        "pan_keytrack" => parse_float_in_range(val, -100.0..=100.0).map(PanKeytrack),
        "pan_veltrack" => parse_float_in_range(val, -100.0..=100.0).map(PanVeltrack),
        "fil_veltrack" => parse_i16_in_range(val, -9600..=9600).map(FilVeltrack),
        "fil_keytrack" => parse_i16_in_range(val, 0..=1200).map(FilKeytrack),
        "fil_keycenter" => parse_key_number(val).map(FilKeycenter),
        "fil_type" => parse_filter_kind(val).map(FilterType),
        "loop_mode" | "loopmode" => parse_loop_mode(val).map(LoopMode),
        "loop_start" | "loopstart" => parse_u32_in_range(val, 0..=u32::MAX).map(LoopStart),
        "loop_end" | "loopend" => parse_u32_in_range(val, 0..=u32::MAX).map(LoopEnd),
        "offset" => parse_u32_in_range(val, 0..=u32::MAX).map(Offset),
        "default_path" => Some(DefaultPath(val.replace('\\', "/"))),
        "tune" => parse_i16_in_range(val, -2400..=2400).map(Tune),

        "ampeg_delay" => parse_float_in_range(val, 0.0..=100.0)
            .map(AmpegDelay)
            .map(AmpegEnvelope),
        "ampeg_start" => parse_float_in_range(val, 0.0..=100.0)
            .map(AmpegStart)
            .map(AmpegEnvelope),
        "ampeg_attack" => parse_float_in_range(val, 0.0..=100.0)
            .map(AmpegAttack)
            .map(AmpegEnvelope),
        "ampeg_hold" => parse_float_in_range(val, 0.0..=100.0)
            .map(AmpegHold)
            .map(AmpegEnvelope),
        "ampeg_decay" => parse_float_in_range(val, 0.0..=100.0)
            .map(AmpegDecay)
            .map(AmpegEnvelope),
        "ampeg_sustain" => parse_float_in_range(val, 0.0..=100.0)
            .map(AmpegSustain)
            .map(AmpegEnvelope),
        "ampeg_release" => parse_float_in_range(val, 0.0..=100.0)
            .map(AmpegRelease)
            .map(AmpegEnvelope),
        "ampeg_vel2release" => parse_float_in_range(val, -100.0..=100.0)
            .map(AmpegVel2Release)
            .map(AmpegEnvelope),

        "sample" => Some(Sample(val.replace('\\', "/"))),

        "amplitude" => parse_float_in_range(val, 0.0..=100.0).map(Amplitude),
        "effect1" => parse_float_in_range(val, 0.0..=100.0).map(ReverbSend),
        "effect2" => parse_float_in_range(val, 0.0..=100.0).map(ChorusSend),
        "xf_cccurve" => match val {
            "power" => Some(XfCcCurve(CrossfadeCurve::Power)),
            "gain" => Some(XfCcCurve(CrossfadeCurve::Gain)),
            _ => None,
        },
        "cutoff_chanaft" => parse_float_in_range(val, -9600.0..=9600.0).map(|value| CcMod {
            target: SfzCcTarget::Cutoff,
            controller: ModController::ChannelPressure,
            value,
        }),

        _ => parse_cc_opcode(name, val),
    })
}

fn parse_sfz_group(group: Group) -> Result<SfzGroupType, SfzValidationError> {
    Ok(match group.name.text {
        "region" => SfzGroupType::Region,
        "group" => SfzGroupType::Group,
        "master" => SfzGroupType::Master,
        "global" => SfzGroupType::Global,
        "control" => SfzGroupType::Control,
        _ => SfzGroupType::Other,
    })
}

fn grammar_token_into_sfz_token(
    token: Token,
    defines: &HashMap<String, String>,
) -> Result<Option<SfzTokenWithMeta>, SfzValidationError> {
    match token.kind {
        TokenKind::Comment(_) => Ok(None),
        TokenKind::Group(group_type) => {
            Ok(Some(SfzTokenWithMeta::Group(parse_sfz_group(group_type)?)))
        }
        TokenKind::Opcode(opcode) => {
            Ok(parse_sfz_opcode(opcode, defines)?.map(SfzTokenWithMeta::Opcode))
        }
        TokenKind::Include(include) => Ok(Some(SfzTokenWithMeta::Import(
            include.path.text.replace('\\', "/"),
        ))),
        TokenKind::Define(define) => {
            let variable = define.variable.text.to_owned();
            let value = define.value.first.value.text.text.to_owned();
            //defines.borrow_mut().insert(variable.clone(), value.clone());
            Ok(Some(SfzTokenWithMeta::Define(variable, value)))
        }
    }
}

#[cfg(test)]
pub fn parse_tokens_raw<'a>(
    input: &'a str,
    defines: &'a HashMap<String, String>,
) -> impl 'a + Iterator<Item = Result<SfzTokenWithMeta, SfzParseError>> {
    let iter = ErrorTolerantToken::parse_as_iter(input);

    iter.filter_map(move |t| match t {
        Ok(t) => match grammar_token_into_sfz_token(t, defines) {
            Ok(Some(t)) => Some(Ok(t)),
            Ok(None) => None,
            Err(e) => Some(Err(SfzParseError::from(e))),
        },
        Err(e) => Some(Err(SfzParseError::from(e))),
    })
}

pub fn parse_tokens_resolved(file_path: &Path) -> Result<Vec<SfzToken>, SfzParseError> {
    TokenResolver::default().resolve_file(file_path)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{
        parse_key_number, parse_tokens_raw, parse_tokens_resolved, SfzToken, SfzTokenWithMeta,
    };

    fn create_temp_dir(label: &str) -> PathBuf {
        let unique = format!(
            "xsynth-sfz-parse-test-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn parse_key_number_accepts_note_names() {
        assert_eq!(parse_key_number("c4"), Some(60));
        assert_eq!(parse_key_number("db3"), Some(49));
        assert_eq!(parse_key_number("-1"), Some(-1));
    }

    #[test]
    fn parse_tokens_raw_resolves_defines_in_opcodes() {
        let defines = HashMap::from([("$VALUE".to_owned(), "snare.wav".to_owned())]);
        let input = "<region>\nsample=$VALUE\n";

        let tokens = parse_tokens_raw(input, &defines)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert!(matches!(tokens[0], SfzTokenWithMeta::Group(_)));
        assert!(matches!(
            tokens[1],
            SfzTokenWithMeta::Opcode(super::SfzOpcode::Sample(ref path)) if path == "snare.wav"
        ));
    }

    #[test]
    fn parse_tokens_resolved_includes_nested_files() {
        let dir = create_temp_dir("resolved");
        let root = dir.join("root.sfz");
        let included = dir.join("nested.sfz");

        fs::write(&included, "<region>\nsample=test.wav\n").unwrap();
        fs::write(&root, "#include \"nested.sfz\"\n").unwrap();

        let tokens = parse_tokens_resolved(&root).unwrap();

        assert!(matches!(
            tokens.as_slice(),
            [SfzToken::Group(_), SfzToken::Opcode(_)]
        ));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn parse_tokens_resolved_reparses_include_after_define_changes() {
        let dir = create_temp_dir("define-reparse");
        let root = dir.join("root.sfz");
        let included = dir.join("nested.sfz");

        fs::write(&included, "<region>\nsample=$NAME\n").unwrap();
        fs::write(
            &root,
            r#"
#define $NAME first.wav
#include "nested.sfz"
#define $NAME second.wav
#include "nested.sfz"
"#,
        )
        .unwrap();

        let tokens = parse_tokens_resolved(&root).unwrap();

        assert!(matches!(
            tokens[1],
            SfzToken::Opcode(super::SfzOpcode::Sample(ref path)) if path == "first.wav"
        ));
        assert!(matches!(
            tokens[3],
            SfzToken::Opcode(super::SfzOpcode::Sample(ref path)) if path == "second.wav"
        ));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn parse_tokens_resolved_rejects_include_cycles() {
        let dir = create_temp_dir("cycle");
        let root = dir.join("root.sfz");
        let nested = dir.join("nested.sfz");

        fs::write(&root, "#include \"nested.sfz\"\n").unwrap();
        fs::write(&nested, "#include \"root.sfz\"\n").unwrap();

        let result = parse_tokens_resolved(&root);
        let root_canonical = root.canonicalize().unwrap();

        assert!(matches!(
            result,
            Err(super::SfzParseError::IncludeCycle(path)) if path == root_canonical
        ));

        fs::remove_dir_all(dir).unwrap();
    }
}
