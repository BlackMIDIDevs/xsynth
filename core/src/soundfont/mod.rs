#![allow(non_camel_case_types)]
use std::{
    collections::{HashMap, HashSet},
    io,
    path::PathBuf,
    sync::Arc,
};

use biquad::Q_BUTTERWORTH_F32;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use thiserror::Error;
use xsynth_soundfonts::{convert_sample_index, FilterType, LoopMode};

use self::audio::load_audio_file;
pub use self::audio::AudioLoadError;

use super::{
    voice::VoiceControlData,
    voice::{EnvelopeParameters, Voice},
};
use crate::{helpers::db_to_amp, voice::EnvelopeDescriptor, AudioStreamParams, ChannelCount};

pub use xsynth_soundfonts::{sf2::Sf2ParseError, sfz::SfzParseError};

mod audio;
mod config;
mod utils;
mod voice_spawners;
use utils::*;
use voice_spawners::*;

pub use config::{Interpolator, SoundfontInitOptions};

pub trait VoiceSpawner: Sync + Send {
    fn spawn_voice(&self, control: &VoiceControlData) -> Box<dyn Voice>;
}

pub trait SoundfontBase: Sync + Send + std::fmt::Debug {
    fn stream_params(&self) -> &'_ AudioStreamParams;

    fn get_attack_voice_spawners_at(
        &self,
        bank: u8,
        preset: u8,
        key: u8,
        vel: u8,
    ) -> Vec<Box<dyn VoiceSpawner>>;
    fn get_release_voice_spawners_at(
        &self,
        bank: u8,
        preset: u8,
        key: u8,
        vel: u8,
    ) -> Vec<Box<dyn VoiceSpawner>>;
}

#[derive(Clone)]
pub(super) struct LoopParams {
    pub mode: LoopMode,
    pub offset: u32,
    pub start: u32,
    pub end: u32,
}

struct SampleVoiceSpawnerParams {
    volume: f32,
    pan: f32,
    speed_mult: f32,
    cutoff: Option<f32>,
    resonance: f32,
    filter_type: FilterType,
    loop_params: LoopParams,
    envelope: Arc<EnvelopeParameters>,
    sample: Arc<[Arc<[f32]>]>,
    interpolator: Interpolator,
}

pub(super) struct SoundfontInstrument {
    bank: u8,
    preset: u8,
    spawner_params_list: Vec<Vec<Arc<SampleVoiceSpawnerParams>>>,
}

/// Represents a sample soundfont to be used within XSynth.
///
/// Supports SFZ and SF2 soundfonts.
///
/// ## SFZ specification support (opcodes)
/// - `lovel` & `hivel`
/// - `lokey` & `hikey`
/// - `pitch_keycenter`
/// - `volume`
/// - `pan`
/// - `sample`
/// - `default_path`
/// - `loop_mode`
/// - `loop_start`
/// - `loop_end`
/// - `offset`
/// - `cutoff`
/// - `resonance`
/// - `fil_veltrack`
/// - `fil_keycenter`
/// - `fil_keytrack`
/// - `filter_type`
/// - `tune`
/// - `ampeg_start`
/// - `ampeg_delay`
/// - `ampeg_attack`
/// - `ampeg_hold`
/// - `ampeg_decay`
/// - `ampeg_sustain`
/// - `ampeg_release`
///
/// ## SF2 specification support
/// ### Generators
/// - `startAddrsOffset`
/// - `startloopAddrsOffset`
/// - `endloopAddrsOffset`
/// - `initialFilterFc`
/// - `initialFilterQ`
/// - `pan`
/// - `delayVolEnv`
/// - `attackVolEnv`
/// - `holdVolEnv`
/// - `decayVolEnv`
/// - `sustainVolEnv`
/// - `releaseVolEnv`
/// - `instrument`
/// - `keyRange`
/// - `velRange`
/// - `initialAttenuation`
/// - `coarseTune`
/// - `fineTune`
/// - `sampleID`
/// - `sampleModes`
/// - `overridingRootKey`
///
/// ### Modulators
/// None
pub struct SampleSoundfont {
    instruments: Vec<SoundfontInstrument>,
    stream_params: AudioStreamParams,
}

/// Errors that can be generated when loading an SFZ soundfont.
#[derive(Debug, Error)]
pub enum LoadSfzError {
    #[error("IO Error")]
    IOError(#[from] io::Error),

    #[error("Error loading samples")]
    AudioLoadError(#[from] AudioLoadError),

    #[error("Error parsing the SFZ: {0}")]
    SfzParseError(#[from] SfzParseError),
}

/// Errors that can be generated when loading a soundfont
/// of an unspecified format.
#[derive(Debug, Error)]
pub enum LoadSfError {
    #[error("Error loading the SFZ: {0}")]
    LoadSfzError(#[from] LoadSfzError),

    #[error("Error loading the SF2: {0}")]
    LoadSf2Error(#[from] Sf2ParseError),

    #[error("Unsupported format")]
    Unsupported,
}

impl SampleSoundfont {
    /// Loads a new sample soundfont of an unspecified type.
    /// The type of the soundfont will be determined from the file extension.
    ///
    /// Parameters:
    /// - `path`: The path of the soundfont to be loaded.
    /// - `stream_params`: Parameters of the output audio. See the `AudioStreamParams`
    ///         documentation for the available options.
    /// - `options`: The soundfont configuration. See the `SoundfontInitOptions`
    ///         documentation for the available options.
    pub fn new(
        path: impl Into<PathBuf>,
        stream_params: AudioStreamParams,
        options: SoundfontInitOptions,
    ) -> Result<Self, LoadSfError> {
        let path: PathBuf = path.into();
        if let Some(ext) = path.extension() {
            match ext.to_str().unwrap_or("").to_lowercase().as_str() {
                "sfz" => {
                    Self::new_sfz(path, stream_params, options).map_err(LoadSfError::LoadSfzError)
                }
                "sf2" => {
                    Self::new_sf2(path, stream_params, options).map_err(LoadSfError::LoadSf2Error)
                }
                _ => Err(LoadSfError::Unsupported),
            }
        } else {
            Err(LoadSfError::Unsupported)
        }
    }

    /// Loads a new SFZ soundfont
    ///
    /// Parameters:
    /// - `path`: The path of the SFZ soundfont to be loaded.
    /// - `stream_params`: Parameters of the output audio. See the `AudioStreamParams`
    ///         documentation for the available options.
    /// - `options`: The soundfont configuration. See the `SoundfontInitOptions`
    ///         documentation for the available options.
    pub fn new_sfz(
        sfz_path: impl Into<PathBuf>,
        stream_params: AudioStreamParams,
        options: SoundfontInitOptions,
    ) -> Result<Self, LoadSfzError> {
        let regions = xsynth_soundfonts::sfz::parse_soundfont(sfz_path.into())?;

        // Find the unique samples that we need to parse and convert
        let unique_sample_params: HashSet<_> = regions
            .iter()
            .map(sample_cache_from_region_params)
            .collect();

        // Parse and convert them in parallel
        let samples: Result<HashMap<_, _>, _> = unique_sample_params
            .into_par_iter()
            .map(|params| -> Result<(_, _), LoadSfzError> {
                let sample = load_audio_file(&params.path, stream_params)?;
                Ok((params, sample))
            })
            .collect();
        let samples = samples?;

        // Find the unique envelope params
        let mut unique_envelope_params =
            Vec::<(EnvelopeDescriptor, Arc<EnvelopeParameters>)>::new();
        for region in regions.iter() {
            let envelope_descriptor =
                envelope_descriptor_from_region_params(&region.ampeg_envelope);
            let exists = unique_envelope_params
                .iter()
                .any(|e| e.0 == envelope_descriptor);
            if !exists {
                unique_envelope_params.push((
                    envelope_descriptor,
                    Arc::new(
                        envelope_descriptor.to_envelope_params(stream_params.sample_rate, options),
                    ),
                ));
            }
        }

        // Generate region params
        let mut spawner_params_list = Vec::<Vec<Arc<SampleVoiceSpawnerParams>>>::new();
        for _ in 0..(128 * 128) {
            spawner_params_list.push(Vec::new());
        }

        // Write region params
        for region in regions {
            let params = sample_cache_from_region_params(&region);
            let envelope = envelope_descriptor_from_region_params(&region.ampeg_envelope);

            // Key value -1 is used for CC triggered regions which are not supported by XSynth
            if region.keyrange.contains(&-1) {
                continue;
            }

            for key in region.keyrange.clone() {
                for vel in region.velrange.clone() {
                    let index = key_vel_to_index(key as u8, vel);
                    let speed_mult =
                        get_speed_mult_from_keys(key as u8, region.pitch_keycenter as u8)
                            * cents_factor(region.tune as f32);

                    let envelope_params = unique_envelope_params
                        .iter()
                        .find(|e| e.0 == envelope)
                        .unwrap()
                        .1
                        .clone();

                    let mut cutoff = None;
                    if options.use_effects {
                        if let Some(mut cutoff_t) = region.cutoff {
                            if cutoff_t >= 1.0 {
                                let cents = vel as f32 / 127.0 * region.fil_veltrack as f32
                                    + (key as f32 - region.fil_keycenter as f32)
                                        * region.fil_keytrack as f32;
                                cutoff_t *= cents_factor(cents);
                                cutoff = Some(
                                    cutoff_t
                                        .clamp(1.0, stream_params.sample_rate as f32 / 2.0 - 100.0),
                                );
                            }
                        }
                    }

                    let pan = ((region.pan as f32 / 100.0) + 1.0) / 2.0;
                    let volume = db_to_amp(region.volume as f32);

                    let sample_rate = samples[&params].1;

                    let loop_params = LoopParams {
                        mode: if region.loop_start == region.loop_end {
                            LoopMode::NoLoop
                        } else {
                            region.loop_mode
                        },
                        offset: convert_sample_index(
                            region.offset,
                            sample_rate,
                            stream_params.sample_rate,
                        ),
                        start: convert_sample_index(
                            region.loop_start,
                            sample_rate,
                            stream_params.sample_rate,
                        ),
                        end: convert_sample_index(
                            region.loop_end,
                            sample_rate,
                            stream_params.sample_rate,
                        ),
                    };

                    let mut region_samples = samples[&params].0.clone();
                    if stream_params.channels == ChannelCount::Stereo && region_samples.len() == 1 {
                        region_samples =
                            Arc::new([region_samples[0].clone(), region_samples[0].clone()]);
                    }

                    let spawner_params = Arc::new(SampleVoiceSpawnerParams {
                        pan,
                        volume,
                        envelope: envelope_params,
                        speed_mult,
                        cutoff,
                        resonance: db_to_amp(region.resonance) * Q_BUTTERWORTH_F32,
                        filter_type: region.filter_type,
                        interpolator: options.interpolator,
                        loop_params,
                        sample: region_samples,
                    });

                    spawner_params_list[index].push(spawner_params.clone());
                }
            }
        }

        Ok(SampleSoundfont {
            instruments: vec![SoundfontInstrument {
                bank: options.bank.unwrap_or(0),
                preset: options.preset.unwrap_or(0),
                spawner_params_list,
            }],
            stream_params,
        })
    }

    /// Loads a new SF2 soundfont
    ///
    /// Parameters:
    /// - `path`: The path of the SF2 soundfont to be loaded.
    /// - `stream_params`: Parameters of the output audio. See the `AudioStreamParams`
    ///         documentation for the available options.
    /// - `options`: The soundfont configuration. See the `SoundfontInitOptions`
    ///         documentation for the available options.
    pub fn new_sf2(
        sf2_path: impl Into<PathBuf>,
        stream_params: AudioStreamParams,
        options: SoundfontInitOptions,
    ) -> Result<Self, Sf2ParseError> {
        let presets =
            xsynth_soundfonts::sf2::load_soundfont(sf2_path.into(), stream_params.sample_rate)?;

        let mut instruments = Vec::new();

        for preset in presets {
            if let Some(bank) = options.bank {
                if bank != preset.bank as u8 {
                    continue;
                }
            }
            if let Some(presetn) = options.preset {
                if presetn != preset.preset as u8 {
                    continue;
                }
            }

            let mut spawner_params_list = Vec::<Vec<Arc<SampleVoiceSpawnerParams>>>::new();
            for _ in 0..(128 * 128) {
                spawner_params_list.push(Vec::new());
            }

            for region in preset.regions {
                let envelope_params = Arc::new(
                    envelope_descriptor_from_region_params(&region.ampeg_envelope)
                        .to_envelope_params(stream_params.sample_rate, options),
                );

                for key in region.keyrange.clone() {
                    for vel in region.velrange.clone() {
                        let index = key_vel_to_index(key, vel);
                        let speed_mult = get_speed_mult_from_keys(key, region.root_key)
                            * cents_factor(
                                region.fine_tune as f32 + region.coarse_tune as f32 * 100.0,
                            );

                        let mut cutoff = None;
                        if options.use_effects {
                            if let Some(cutoff_t) = region.cutoff {
                                if cutoff_t >= 1.0 {
                                    cutoff = Some(cutoff_t.clamp(
                                        1.0,
                                        stream_params.sample_rate as f32 / 2.0 - 100.0,
                                    ));
                                }
                            }
                        }

                        let pan = ((region.pan as f32 / 500.0) + 1.0) / 2.0;

                        let loop_params = LoopParams {
                            mode: if region.loop_start == region.loop_end {
                                LoopMode::NoLoop
                            } else {
                                region.loop_mode
                            },
                            offset: region.offset,
                            start: region.loop_start,
                            end: region.loop_end,
                        };

                        let mut region_samples = region.sample.clone();
                        if stream_params.channels == ChannelCount::Stereo
                            && region_samples.len() == 1
                        {
                            region_samples =
                                Arc::new([region_samples[0].clone(), region_samples[0].clone()]);
                        }

                        let spawner_params = Arc::new(SampleVoiceSpawnerParams {
                            pan,
                            volume: region.volume,
                            envelope: envelope_params.clone(),
                            speed_mult,
                            cutoff,
                            resonance: db_to_amp(region.resonance) * Q_BUTTERWORTH_F32,
                            filter_type: FilterType::LowPass,
                            interpolator: options.interpolator,
                            loop_params,
                            sample: region_samples,
                        });

                        spawner_params_list[index].push(spawner_params.clone());
                    }
                }
            }

            let new = SoundfontInstrument {
                bank: preset.bank as u8,
                preset: preset.preset as u8,
                spawner_params_list,
            };
            instruments.push(new);
        }

        Ok(SampleSoundfont {
            instruments,
            stream_params,
        })
    }
}

impl std::fmt::Debug for SampleSoundfont {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "SampleSoundfont")
    }
}

impl SoundfontBase for SampleSoundfont {
    fn stream_params(&self) -> &'_ AudioStreamParams {
        &self.stream_params
    }

    fn get_attack_voice_spawners_at(
        &self,
        bank: u8,
        preset: u8,
        key: u8,
        vel: u8,
    ) -> Vec<Box<dyn VoiceSpawner>> {
        use simdeez::*; // nuts

        use simdeez::prelude::*;

        simd_runtime_generate!(
            fn get(
                key: u8,
                vel: u8,
                sf: &SoundfontInstrument,
                stream_params: &AudioStreamParams,
            ) -> Vec<Box<dyn VoiceSpawner>> {
                if sf.spawner_params_list.is_empty() {
                    return Vec::new();
                }

                let index = key_vel_to_index(key, vel);
                let mut vec = Vec::<Box<dyn VoiceSpawner>>::new();
                for spawner in &sf.spawner_params_list[index] {
                    match stream_params.channels {
                        ChannelCount::Stereo => vec.push(Box::new(
                            StereoSampledVoiceSpawner::<S>::new(spawner, vel, *stream_params),
                        )),
                        ChannelCount::Mono => vec.push(Box::new(
                            MonoSampledVoiceSpawner::<S>::new(spawner, vel, *stream_params),
                        )),
                    }
                }
                vec
            }
        );

        let empty = SoundfontInstrument {
            bank: 0,
            preset: 0,
            spawner_params_list: Vec::new(),
        };

        let instrument = self
            .instruments
            .iter()
            .find(|i| i.bank == bank && i.preset == preset)
            .unwrap_or(&empty);

        get(key, vel, instrument, self.stream_params())
    }

    fn get_release_voice_spawners_at(
        &self,
        _bank: u8,
        _preset: u8,
        _key: u8,
        _vel: u8,
    ) -> Vec<Box<dyn VoiceSpawner>> {
        vec![]
    }
}
