use crate::{sfz::AmpegEnvelopeParams, LoopMode};
use soundfont::raw::SampleLink;
use std::{fs::File, ops::RangeInclusive, path::PathBuf, sync::Arc};

use thiserror::Error;

mod instrument;
mod modulator;
mod preset;
mod sample;
mod zone;

pub use modulator::Sf2NoteParams;
pub(crate) use modulator::{
    concave_lookup, convex_lookup, default_note_modulators, default_raw_envelope, Sf2NoteModulator,
    Sf2RawEnvelope,
};

/// Errors that can be generated when loading an SF2 file.
#[derive(Error, Debug, Clone)]
pub enum Sf2ParseError {
    #[error("Failed to read file: {0}")]
    FailedToReadFile(PathBuf),

    #[error("Failed to parse file")]
    FailedToParseFile(String),
}

/// Structure that holds the generator and modulator parameters of an SF2 region.
///
/// XSynth's SF2 support is intentionally performance-first. Static region data
/// and a practical subset of note-on modulators are resolved during load so the
/// realtime engine can keep using pre-specialized voice paths. Modulators
/// driven by MIDI CCs, channel pressure or the pitch wheel are exposed through
/// `Sf2Region::runtime_modulation`. Modulation envelopes, LFO-driven modulation
/// and per-region chorus/reverb sends are intentionally omitted.
#[derive(Clone, Debug)]
pub struct Sf2Region {
    pub sample: Arc<[Arc<[f32]>]>,
    pub sample_rate: u32,
    pub velrange: RangeInclusive<u8>,
    pub keyrange: RangeInclusive<u8>,
    pub root_key: u8,
    pub volume: f32,
    pub pan: i16,
    pub loop_mode: LoopMode,
    pub loop_start: u32,
    pub loop_end: u32,
    pub offset: u32,
    pub sample_end: u32,
    pub cutoff: Option<f32>,
    pub resonance: f32,
    pub ampeg_envelope: AmpegEnvelopeParams,
    pub fine_tune: i16,
    pub coarse_tune: i16,
    pub scale_tuning: i16,
    pub exclusive_class: Option<u8>,
    pub(crate) keynum_to_vol_env_hold: i16,
    pub(crate) keynum_to_vol_env_decay: i16,
    pub(crate) cutoff_cents: Option<i32>,
    pub(crate) raw_envelope: Sf2RawEnvelope,
    pub(crate) note_modulators: Arc<[Sf2NoteModulator]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sf2SampleLinkType {
    Mono,
    Left,
    Right,
    Linked,
}

impl From<SampleLink> for Sf2SampleLinkType {
    fn from(value: SampleLink) -> Self {
        if value.is_left() {
            Self::Left
        } else if value.is_right() {
            Self::Right
        } else if value.is_linked() {
            Self::Linked
        } else {
            Self::Mono
        }
    }
}

/// Structure that holds the parameters of an SF2 preset.
#[derive(Clone, Debug)]
pub struct Sf2Preset {
    pub bank: u16,
    pub preset: u16,
    pub regions: Vec<Sf2Region>,
}

/// Parses an SF2 file and returns its presets in a vector.
///
/// Supported behavior includes sample offsets/loops, stereo links, static
/// filter and tuning generators, volume envelope generators, exclusive class,
/// key/velocity ranges, a baked subset of key/velocity note-on modulators, and
/// controller-driven modulators for attenuation, pan, filter, tuning and envelopes.
pub fn load_soundfont(
    sf2_path: impl Into<PathBuf>,
    sample_rate: u32,
) -> Result<Vec<Sf2Preset>, Sf2ParseError> {
    let sf2_path: PathBuf = sf2_path.into();
    let sf2_path: PathBuf = sf2_path
        .canonicalize()
        .map_err(|_| Sf2ParseError::FailedToReadFile(sf2_path.clone()))?;
    let mut file = File::open(sf2_path.clone())
        .map_err(|_| Sf2ParseError::FailedToReadFile(sf2_path.clone()))?;
    let file = &mut file;
    let sf2 = soundfont::SoundFont2::load(file)
        .map_err(|e| Sf2ParseError::FailedToParseFile(format!("{e:#?}")))?
        .sort_presets();

    let sample_data = sample::Sf2Sample::parse_sf2_samples(
        file,
        sf2.sample_headers,
        sf2.sample_data,
        sample_rate,
    )?;

    let instruments = instrument::Sf2Instrument::parse_instruments(sf2.instruments);

    let presets = preset::Sf2ParsedPreset::parse_presets(sf2.presets);

    Ok(preset::Sf2ParsedPreset::merge_presets(
        sample_data,
        instruments,
        presets,
        sample_rate,
    ))
}
