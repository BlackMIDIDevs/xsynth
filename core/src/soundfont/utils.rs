use crate::{helpers::FREQS, voice::EnvelopeDescriptor};
use std::path::PathBuf;
use xsynth_soundfonts::sfz::{AmpegEnvelopeParams, RegionParams};

#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) struct SampleCache {
    pub(super) path: PathBuf,
}

impl SampleCache {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

pub(super) fn get_speed_mult_from_keys(key: u8, base_key: u8) -> f32 {
    let base_freq = FREQS[base_key as usize];
    let freq = FREQS[key as usize];
    freq / base_freq
}

pub(super) fn key_vel_to_index(key: u8, vel: u8) -> usize {
    (key as usize) * 128 + (vel as usize)
}

pub(super) fn cents_factor(cents: f32) -> f32 {
    2.0f32.powf(cents / 1200.0)
}

pub(super) fn sample_cache_from_region_params(region_params: &RegionParams) -> SampleCache {
    SampleCache::new(region_params.sample_path.clone())
}

pub(super) fn envelope_descriptor_from_region_params(
    region_params: &AmpegEnvelopeParams,
) -> EnvelopeDescriptor {
    let env = region_params;
    EnvelopeDescriptor {
        start_percent: env.ampeg_start / 100.0,
        delay: env.ampeg_delay,
        attack: env.ampeg_attack,
        hold: env.ampeg_hold,
        decay: env.ampeg_decay,
        sustain_percent: env.ampeg_sustain / 100.0,
        release: env.ampeg_release,
    }
}
