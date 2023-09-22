use std::{path::PathBuf, fs::File, sync::Arc, ops::RangeInclusive,};
use crate::{LoopMode, sfz::AmpegEnvelopeParams};

use thiserror::Error;
use soundfont;

mod sample;
mod instrument;
mod preset;

#[derive(Error, Debug, Clone)]
pub enum Sf2ParseError {
    #[error("Failed to read file: {0}")]
    FailedToReadFile(PathBuf),

    #[error("Failed to parse file")]
    FailedToParseFile,
}

pub struct Sf2Region {
    pub sample: Arc<[i32]>,
    pub velrange: RangeInclusive<u8>,
    pub keyrange: RangeInclusive<i8>,
    pub root_key: i8,
    pub volume: i16,
    pub pan: i8,
    pub loop_mode: LoopMode,
    pub loop_start: u32,
    pub loop_end: u32,
    pub cutoff: Option<f32>,
    pub resonance: f32,
    pub ampeg_envelope: AmpegEnvelopeParams,
    pub tune: i16,
}

pub struct Sf2Preset {
    pub bank: u16,
    pub preset: u16,
    pub regions: Vec<Sf2Region>,
}

pub fn load_soundfont(sf2_path: impl Into<PathBuf>) -> Result<Vec<Sf2Preset>, Sf2ParseError> {
    let sf2_path: PathBuf = sf2_path.into();
    let sf2_path: PathBuf = sf2_path
        .canonicalize()
        .map_err(|_| Sf2ParseError::FailedToReadFile(sf2_path.clone()))?;
    let mut file = File::open(sf2_path.clone())
        .map_err(|_| Sf2ParseError::FailedToReadFile(sf2_path.clone()))?;
    let file = &mut file;
    let sf2 = soundfont::SoundFont2::load(file).map_err(|_| Sf2ParseError::FailedToParseFile)?.sort_presets();

    let sample_data = sample::parse_sf2_samples(file, sf2.sample_headers, sf2.sample_data);

    Ok(vec![])
}
