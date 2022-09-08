use std::{
    collections::{HashMap, HashSet},
    io,
    marker::PhantomData,
    path::PathBuf,
    sync::Arc,
};

use rayon::iter::{IntoParallelIterator, ParallelIterator};
use simdeez::Simd;
use soundfonts::sfz::RegionParams;
use thiserror::Error;

use self::audio::{load_audio_file, AudioLoadError};

use super::{
    voice::VoiceControlData,
    voice::{
        BufferSamplers, EnvelopeParameters, SIMDConstant, SIMDNearestSampleGrabber,
        SIMDStereoVoice, SIMDStereoVoiceSampler, SIMDVoiceControl, SIMDVoiceEnvelope, SampleReader,
        Voice, VoiceBase, VoiceCombineSIMD,
    },
};
use crate::{helpers::FREQS, voice::EnvelopeDescriptor, AudioStreamParams};

pub mod audio;

pub trait VoiceSpawner: Sync + Send {
    fn spawn_voice(&self, control: &VoiceControlData) -> Box<dyn Voice>;
}

pub trait SoundfontBase: Sync + Send + std::fmt::Debug {
    fn stream_params<'a>(&'a self) -> &'a AudioStreamParams;

    fn get_attack_voice_spawners_at(&self, key: u8, vel: u8) -> Vec<Box<dyn VoiceSpawner>>;
    fn get_release_voice_spawners_at(&self, key: u8, vel: u8) -> Vec<Box<dyn VoiceSpawner>>;
}

struct SampleVoiceSpawnerParams {
    speed_mult: f32,
    cutoff: f32,
    envelope: Arc<EnvelopeParameters>,
    sample: Vec<Arc<[f32]>>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct SampleCache {
    path: PathBuf,
}

fn get_speed_mult_from_keys(key: u8, base_key: u8) -> f32 {
    let base_freq = FREQS[base_key as usize];
    let freq = FREQS[key as usize];
    freq / base_freq
}

impl SampleCache {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

struct SampledVoiceSpawner<S: 'static + Simd + Send + Sync> {
    speed_mult: f32,
    cutoff: f32,
    amp: f32,
    volume_envelope_params: Arc<EnvelopeParameters>,
    samples: Vec<Arc<[f32]>>,
    vel: u8,
    _s: PhantomData<S>,
}

impl<S: Simd + Send + Sync> SampledVoiceSpawner<S> {
    pub fn new(params: &SampleVoiceSpawnerParams, vel: u8) -> Self {
        let amp = 1.04f32.powf(vel as f32 - 127.0);

        Self {
            speed_mult: params.speed_mult,
            cutoff: params.cutoff,
            amp,
            volume_envelope_params: params.envelope.clone(),
            samples: params.sample.clone(),
            vel,
            _s: PhantomData,
        }
    }
}

impl<S: 'static + Sync + Send + Simd> VoiceSpawner for SampledVoiceSpawner<S> {
    fn spawn_voice(&self, control: &VoiceControlData) -> Box<dyn Voice> {
        let pitch_fac = SIMDConstant::<S>::new(self.speed_mult as f32);

        let pitch_multiplier = SIMDVoiceControl::new(control, |vc| vc.voice_pitch_multiplier);

        let pitch_fac = VoiceCombineSIMD::mult(pitch_fac, pitch_multiplier);

        let cutoff = SIMDConstant::<S>::new(self.cutoff);

        let left = SIMDNearestSampleGrabber::new(SampleReader::new(BufferSamplers::new_f32(
            self.samples[0].clone(),
        )));
        let right = SIMDNearestSampleGrabber::new(SampleReader::new(BufferSamplers::new_f32(
            self.samples[1].clone(),
        )));

        let sampler = SIMDStereoVoiceSampler::new(left, right, pitch_fac, cutoff);

        let amp = SIMDConstant::<S>::new(self.amp);
        let volume_envelope = SIMDVoiceEnvelope::new(self.volume_envelope_params.clone());

        let modulated = VoiceCombineSIMD::mult(amp, sampler);
        let modulated = VoiceCombineSIMD::mult(volume_envelope, modulated);

        let flattened = SIMDStereoVoice::new(modulated);
        let base = VoiceBase::new(self.vel, flattened);

        Box::new(base)
    }
}

fn key_vel_to_index(key: u8, vel: u8) -> usize {
    (key as usize) * 128 + (vel as usize)
}

pub struct SampleSoundfont {
    spawner_params_list: Vec<Option<Arc<SampleVoiceSpawnerParams>>>,
    stream_params: AudioStreamParams,
}

fn sample_cache_from_region_params(region_params: &RegionParams) -> SampleCache {
    SampleCache::new(region_params.sample_path.clone())
}

fn envelope_descriptor_from_region_params(region_params: &RegionParams) -> EnvelopeDescriptor {
    let env = &region_params.ampeg_envelope;
    EnvelopeDescriptor {
        start_percent: env.ampeg_start / 100.0,
        delay: env.ampeg_delay,
        attack: env.ampeg_attack,
        hold: env.ampeg_hold,
        decay: env.ampeg_decay,
        sustain_percent: env.ampeg_sustain / 100.0,
        release: env.ampeg_release.min(0.3),
    }
}

#[derive(Debug, Error)]
pub enum LoadSfzError {
    #[error("IO Error")]
    IOError(#[from] io::Error),

    #[error("Error loading samples")]
    AudioLoadError(#[from] AudioLoadError),
}

impl SampleSoundfont {
    pub fn new(
        sfz_path: impl Into<PathBuf>,
        stream_params: AudioStreamParams,
    ) -> Result<Self, LoadSfzError> {
        let regions = soundfonts::sfz::parse_soundfont(sfz_path.into())?;

        // Find the unique samples that we need to parse and convert
        let unique_sample_params: HashSet<_> = regions
            .iter()
            .map(sample_cache_from_region_params)
            .collect();

        // Parse and convert them in parallel
        let samples: Result<HashMap<_, _>, _> = unique_sample_params
            .into_par_iter()
            .map(|params| -> Result<(_, _), LoadSfzError> {
                let sample = load_audio_file(&params.path, stream_params.sample_rate as f32)?;
                Ok((params, sample))
            })
            .collect();
        let samples = samples?;

        // Find the unique envelope params
        let mut unique_envelope_params =
            Vec::<(EnvelopeDescriptor, Arc<EnvelopeParameters>)>::new();
        for region in regions.iter() {
            let envelope_descriptor = envelope_descriptor_from_region_params(&region);
            let exists = unique_envelope_params
                .iter()
                .find(|e| &e.0 == &envelope_descriptor)
                .is_some();
            if !exists {
                unique_envelope_params.push((
                    envelope_descriptor,
                    Arc::new(envelope_descriptor.to_envelope_params(stream_params.sample_rate)),
                ));
            }
        }

        // Generate region params
        let mut spawner_params_list = Vec::<Option<Arc<SampleVoiceSpawnerParams>>>::new();
        for _ in 0..(128 * 128) {
            spawner_params_list.push(None);
        }

        // Write region params
        for region in regions {
            let params = sample_cache_from_region_params(&region);
            let envelope = envelope_descriptor_from_region_params(&region);

            if region.lokey.is_none() && region.hikey.is_none() {
                for vel in region.lovel..=region.hivel {
                    let index = key_vel_to_index(region.key.unwrap(), vel);

                    let speed_mult = get_speed_mult_from_keys(region.key.unwrap(), region.pitch_keycenter.unwrap_or(region.key.unwrap()));

                    let envelope_params = unique_envelope_params
                        .iter()
                        .find(|e| &e.0 == &envelope)
                        .unwrap()
                        .1
                        .clone();

                    let cutoff = region.cutoff; // TODO: fil_veltrack

                    let spawner_params = Arc::new(SampleVoiceSpawnerParams {
                        envelope: envelope_params,
                        speed_mult,
                        cutoff,
                        sample: samples[&params].clone(),
                    });

                    spawner_params_list[index] = Some(spawner_params.clone());
                }
            } else {
                let lokey = region.lokey.unwrap();
                let hikey = region.hikey.unwrap();
                for k in lokey..=hikey {
                    for vel in region.lovel..=region.hivel {
                        let index = key_vel_to_index(k, vel);

                        let center = if region.pitch_keycenter == None && region.key != None {
                            region.key.unwrap()
                        } else {
                            region.pitch_keycenter.unwrap_or(k)
                        };

                        let speed_mult = get_speed_mult_from_keys(k, center);

                        let envelope_params = unique_envelope_params
                            .iter()
                            .find(|e| &e.0 == &envelope)
                            .unwrap()
                            .1
                            .clone();

                        let cutoff = region.cutoff; // TODO: fil_veltrack

                        let spawner_params = Arc::new(SampleVoiceSpawnerParams {
                            envelope: envelope_params,
                            speed_mult,
                            cutoff,
                            sample: samples[&params].clone(),
                        });

                        spawner_params_list[index] = Some(spawner_params.clone());
                    }
                }
            }
        }

        Ok(SampleSoundfont {
            spawner_params_list,
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
    fn stream_params<'a>(&'a self) -> &'a AudioStreamParams {
        &self.stream_params
    }

    fn get_attack_voice_spawners_at(&self, key: u8, vel: u8) -> Vec<Box<dyn VoiceSpawner>> {
        use simdeez::*; // nuts

        use simdeez::avx2::*;
        use simdeez::scalar::*;
        use simdeez::sse2::*;
        use simdeez::sse41::*;

        simd_runtime_generate!(
            fn get(key: u8, vel: u8, sf: &SampleSoundfont) -> Vec<Box<dyn VoiceSpawner>> {
                let index = key_vel_to_index(key, vel);
                let spawner_params = sf.spawner_params_list[index].as_ref();
                if let Some(spawner_params) = spawner_params {
                    vec![Box::new(SampledVoiceSpawner::<S>::new(spawner_params, vel))]
                } else {
                    vec![]
                }
            }
        );

        get_runtime_select(key, vel, &self)
    }

    fn get_release_voice_spawners_at(&self, _key: u8, _vel: u8) -> Vec<Box<dyn VoiceSpawner>> {
        vec![]
    }
}
