use crate::{sfz::AmpegEnvelopeParams, LoopMode};
use std::{fs::File, ops::RangeInclusive, path::PathBuf, sync::Arc};

use thiserror::Error;

mod instrument;
mod preset;
mod sample;

#[derive(Error, Debug, Clone)]
pub enum Sf2ParseError {
    #[error("Failed to read file: {0}")]
    FailedToReadFile(PathBuf),

    #[error("Failed to parse file")]
    FailedToParseFile,
}

#[derive(Default, Clone, Debug)]
struct Sf2Zone {
    pub index: Option<u16>,
    pub offset: Option<i16>,
    pub loop_start_offset: Option<i16>,
    pub loop_end_offset: Option<i16>,
    pub loop_mode: Option<LoopMode>,
    pub cutoff: Option<i16>,
    pub resonance: Option<i16>,
    pub pan: Option<i16>,
    pub env_delay: Option<f32>,
    pub env_attack: Option<f32>,
    pub env_hold: Option<f32>,
    pub env_decay: Option<f32>,
    pub env_sustain: Option<f32>,
    pub env_release: Option<f32>,
    pub velrange: Option<RangeInclusive<u8>>,
    pub keyrange: Option<RangeInclusive<u8>>,
    pub attenuation: Option<i16>,
    pub fine_tune: Option<i16>,
    pub coarse_tune: Option<i16>,
    pub root_override: Option<i16>,
}

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
    pub cutoff: Option<f32>,
    pub resonance: f32,
    pub ampeg_envelope: AmpegEnvelopeParams,
    pub fine_tune: i16,
    pub coarse_tune: i16,
}

pub struct Sf2Preset {
    pub bank: u16,
    pub preset: u16,
    pub regions: Vec<Sf2Region>,
}

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
        .map_err(|_| Sf2ParseError::FailedToParseFile)?
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
    ))
}