use std::{
    borrow::Cow,
    cell::RefCell,
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use crate::FilterType;

use super::grammar::{
    helpers::{FileLocation, ParseError},
    ErrorTolerantToken, Group, Opcode, Token, TokenKind,
};
use thiserror::Error;

#[derive(Debug, Clone)]
pub enum SfzLoopMode {
    NoLoop,
    OneShot,
    LoopContinuous,
    LoopSustain,
}

#[derive(Debug, Clone)]
pub enum SfzOpcode {
    Lovel(u8),
    Hivel(u8),
    Key(u8),
    Lokey(u8),
    Hikey(u8),
    PitchKeycenter(u8),
    Pan(i8),
    Sample(String),
    LoopMode(SfzLoopMode),
    Cutoff(f32),
    FilVeltrack(i16),
    FilKeycenter(u8),
    FilKeytrack(i16),
    FilterType(FilterType),
    DefaultPath(String),
    AmpegEnvelope(SfzAmpegEnvelope),
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SfzGroupType {
    Region,
    Group,
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
    // TODO: Add #defines here
}

#[derive(Error, Debug, Clone)]
pub struct SfzValidationError {
    pub pos: FileLocation,
    pub message: String,
}

impl SfzValidationError {
    pub fn new(pos: FileLocation, message: String) -> Self {
        Self { pos, message }
    }
}

impl std::fmt::Display for SfzValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {}", self.message, self.pos)
    }
}

#[derive(Error, Debug, Clone)]
pub enum SfzParseError {
    #[error("Failed to parse SFZ file: {0}")]
    GrammarError(#[from] ParseError),

    #[error("Failed to parse SFZ file: {0}")]
    ValidationError(#[from] SfzValidationError),

    #[error("Failed to read file: {0}")]
    FailedToReadFile(PathBuf),
}

fn parse_key_number(val: &str) -> Option<u8> {
    match val.parse().ok() {
        Some(val) => Some(val),
        None => {
            let note: String = val
                .chars()
                .filter(|c| !(c.is_ascii_digit() || c == &'-'))
                .collect();
            let semitone: i8 = match note.to_lowercase().as_str() {
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
            let octave: i8 = octave.parse().ok().unwrap_or(-10);
            if octave < -1 {
                None
            } else {
                let midi_note = 12 + semitone + octave * 12;
                Some(midi_note as u8)
            }
        }
    }
}

fn parse_vel_number(val: &str) -> Option<u8> {
    val.parse().ok()
}

fn parse_pan_number(val: &str) -> Option<i8> {
    val.parse().ok()
}

fn parse_i16(val: &str) -> Option<i16> {
    val.parse().ok()
}

fn parse_float(val: &str) -> Option<f32> {
    val.parse().ok()
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

fn parse_loop_mode(val: &str) -> Option<SfzLoopMode> {
    match val {
        "no_loop" => Some(SfzLoopMode::NoLoop),
        "one_shot" => Some(SfzLoopMode::OneShot),
        "loop_continuous" => Some(SfzLoopMode::LoopContinuous),
        "loop_sustain" => Some(SfzLoopMode::LoopSustain),
        _ => None,
    }
}

fn parse_sfz_opcode(
    opcode: Opcode,
    defines: &RefCell<HashMap<String, String>>,
) -> Result<Option<SfzOpcode>, SfzValidationError> {
    let val = opcode.value.as_string();
    let mut val = Cow::Borrowed(val.trim());

    for (key, replace) in defines.borrow().iter() {
        if val.contains(key) {
            val = Cow::Owned(val.replace(key, replace));
        }
    }

    use SfzAmpegEnvelope::*;
    use SfzOpcode::*;

    let val = val.as_ref();

    Ok(match opcode.name.name.text {
        "lokey" => parse_key_number(val).map(Lokey),
        "hikey" => parse_key_number(val).map(Lokey),
        "lovel" => parse_vel_number(val).map(Lovel),
        "hivel" => parse_vel_number(val).map(Lovel),
        "pan" => parse_pan_number(val).map(Pan),
        "pitch_keycenter" => parse_key_number(val).map(PitchKeycenter),
        "key" => parse_key_number(val).map(Key),
        "cutoff" => parse_float(val).map(Cutoff),
        "fil_veltrack" => parse_i16(val).map(FilVeltrack),
        "fil_keytrack" => parse_i16(val).map(FilKeytrack),
        "fil_keycenter" => parse_key_number(val).map(FilKeycenter),
        "fil_type" => parse_filter_kind(val).map(FilterType),
        "loop_mode" => parse_loop_mode(val).map(LoopMode),
        "default_path" => Some(DefaultPath(val.to_string())),

        "ampeg_delay" => parse_float(val).map(AmpegDelay).map(AmpegEnvelope),
        "ampeg_start" => parse_float(val).map(AmpegStart).map(AmpegEnvelope),
        "ampeg_attack" => parse_float(val).map(AmpegAttack).map(AmpegEnvelope),
        "ampeg_hold" => parse_float(val).map(AmpegHold).map(AmpegEnvelope),
        "ampeg_decay" => parse_float(val).map(AmpegDecay).map(AmpegEnvelope),
        "ampeg_sustain" => parse_float(val).map(AmpegSustain).map(AmpegEnvelope),
        "ampeg_release" => parse_float(val).map(AmpegRelease).map(AmpegEnvelope),

        "sample" => Some(Sample(val.replace('\\', "/"))),

        _ => None,
    })
}

fn parse_sfz_group(group: Group) -> Result<SfzGroupType, SfzValidationError> {
    Ok(match group.name.text {
        "region" => SfzGroupType::Region,
        "group" => SfzGroupType::Group,
        "master" => SfzGroupType::Global,
        "control" => SfzGroupType::Control,
        "global" => SfzGroupType::Global,
        _ => SfzGroupType::Other,
    })
}

fn grammar_token_into_sfz_token(
    token: Token,
    defines: &RefCell<HashMap<String, String>>,
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
    }
}

pub fn parse_tokens_raw<'a>(
    input: &'a str,
    defines: &'a RefCell<HashMap<String, String>>,
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

fn parse_tokens_resolved_recursive(
    file_path: &Path,
    defines: &RefCell<HashMap<String, String>>,
) -> Result<Vec<SfzToken>, SfzParseError> {
    let file_path = file_path
        .canonicalize()
        .map_err(|_| SfzParseError::FailedToReadFile(file_path.to_owned()))?;
    let file = fs::read_to_string(&file_path)
        .map_err(|_| SfzParseError::FailedToReadFile(file_path.to_owned()))?;

    // Unwrap here is safe because the path is confirmed to be a file (read above)
    // and therefore it will always have a parent folder. The path is also canonicalized.
    let parent_path = file_path.parent().unwrap();

    let mut tokens = Vec::new();

    let iter = parse_tokens_raw(&file, defines);

    for t in iter {
        match t {
            Ok(t) => match t {
                SfzTokenWithMeta::Import(path) => {
                    let full_path = parent_path.join(path);
                    let mut parsed_tokens = parse_tokens_resolved_recursive(&full_path, defines)?;
                    tokens.append(&mut parsed_tokens);
                }
                SfzTokenWithMeta::Group(group) => tokens.push(SfzToken::Group(group)),
                SfzTokenWithMeta::Opcode(opcode) => tokens.push(SfzToken::Opcode(opcode)),
                // TODO: insert new #defines here
            },
            Err(e) => return Err(e),
        }
    }

    Ok(tokens)
}

pub fn parse_tokens_resolved(file_path: &Path) -> Result<Vec<SfzToken>, SfzParseError> {
    let defines = RefCell::new(HashMap::new());
    parse_tokens_resolved_recursive(file_path, &defines)
}
