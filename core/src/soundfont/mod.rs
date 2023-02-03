use std::{
    collections::{HashMap, HashSet},
    io,
    marker::PhantomData,
    ops::Mul,
    path::PathBuf,
    sync::Arc,
};

use rayon::iter::{IntoParallelIterator, ParallelIterator};
use simdeez::Simd;
use soundfonts::sfz::{parse::SfzParseError, RegionParams};
use thiserror::Error;

use self::audio::{load_audio_file, AudioLoadError};

use super::{
    voice::VoiceControlData,
    voice::{
        BufferSamplers, EnvelopeParameters, SIMDConstant, SIMDConstantStereo,
        SIMDNearestSampleGrabber, SIMDStereoVoice, SIMDStereoVoiceSampler, SIMDVoiceControl,
        SIMDVoiceEnvelope, SampleReader, Voice, VoiceBase, VoiceCombineSIMD,
    },
};
use crate::{
    effects::BiQuadFilter,
    helpers::FREQS,
    voice::{
        EnvelopeDescriptor, SIMDSample, SIMDSampleMono, SIMDSampleStereo, SIMDStereoVoiceCutoff,
        SIMDVoiceGenerator,
    },
    AudioStreamParams, ChannelCount,
};

use soundfonts::FilterType;

pub mod audio;

pub trait VoiceSpawner: Sync + Send {
    fn spawn_voice(&self, control: &VoiceControlData) -> Box<dyn Voice>;
}

pub trait SoundfontBase: Sync + Send + std::fmt::Debug {
    fn stream_params(&self) -> &'_ AudioStreamParams;

    fn get_attack_voice_spawners_at(&self, key: u8, vel: u8) -> Vec<Box<dyn VoiceSpawner>>;
    fn get_release_voice_spawners_at(&self, key: u8, vel: u8) -> Vec<Box<dyn VoiceSpawner>>;
}

struct SampleVoiceSpawnerParams {
    volume: f32,
    pan: f32,
    speed_mult: f32,
    cutoff: Option<f32>,
    filter_type: FilterType,
    envelope: Arc<EnvelopeParameters>,
    sample: Arc<[Arc<[f32]>]>,
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
    filter: Option<BiQuadFilter>,
    amp: f32,
    pan: f32,
    volume_envelope_params: Arc<EnvelopeParameters>,
    samples: Arc<[Arc<[f32]>]>,
    vel: u8,
    stream_params: AudioStreamParams,
    _s: PhantomData<S>,
}

impl<S: Simd + Send + Sync> SampledVoiceSpawner<S> {
    pub fn new(
        params: &SampleVoiceSpawnerParams,
        vel: u8,
        stream_params: AudioStreamParams,
    ) -> Self {
        let amp = (vel as f32 / 127.0).powi(2) * params.volume;

        let filter = params.cutoff.map(|cutoff| {
            BiQuadFilter::new(params.filter_type, cutoff, stream_params.sample_rate as f32)
        });

        Self {
            speed_mult: params.speed_mult,
            filter,
            amp,
            pan: params.pan,
            volume_envelope_params: params.envelope.clone(),
            samples: params.sample.clone(),
            vel,
            stream_params,
            _s: PhantomData,
        }
    }

    fn create_pitch_fac(
        &self,
        control: &VoiceControlData,
    ) -> impl SIMDVoiceGenerator<S, SIMDSampleMono<S>> {
        let pitch_fac = SIMDConstant::<S>::new(self.speed_mult);
        let pitch_multiplier = SIMDVoiceControl::new(control, |vc| vc.voice_pitch_multiplier);
        let pitch_fac = VoiceCombineSIMD::mult(pitch_fac, pitch_multiplier);
        pitch_fac
    }

    fn get_sampler(
        &self,
        control: &VoiceControlData,
    ) -> impl SIMDVoiceGenerator<S, SIMDSampleStereo<S>> {
        let left = SIMDNearestSampleGrabber::new(SampleReader::new(BufferSamplers::new_f32(
            self.samples[0].clone(),
        )));
        let right = SIMDNearestSampleGrabber::new(SampleReader::new(BufferSamplers::new_f32(
            self.samples[1].clone(),
        )));

        let pitch_fac = self.create_pitch_fac(control);

        let sampler = SIMDStereoVoiceSampler::new(left, right, pitch_fac);
        sampler
    }

    fn apply_velocity<Gen, Sample>(&self, gen: Gen) -> impl SIMDVoiceGenerator<S, Sample>
    where
        Sample: SIMDSample<S>,
        SIMDSampleMono<S>: Mul<Sample, Output = Sample>,
        Gen: SIMDVoiceGenerator<S, Sample>,
    {
        let amp = SIMDConstant::<S>::new(self.amp);
        let amp = VoiceCombineSIMD::mult(amp, gen);
        amp
    }

    fn apply_pan<Gen, Sample>(&self, gen: Gen) -> impl SIMDVoiceGenerator<S, Sample>
    where
        Sample: SIMDSample<S>,
        SIMDSampleStereo<S>: Mul<Sample, Output = Sample>,
        Gen: SIMDVoiceGenerator<S, Sample>,
    {
        let pan = self.pan * std::f32::consts::PI / 2.0;
        let leftg = (pan.cos() * 1.42).min(1.0);
        let rightg = (pan.sin() * 1.42).min(1.0);

        let gains = SIMDConstantStereo::<S>::new(leftg, rightg);

        let panned = VoiceCombineSIMD::mult(gains, gen);
        panned
    }

    fn apply_envelope<Gen, Sample>(
        &self,
        gen: Gen,
        control: &VoiceControlData,
    ) -> impl SIMDVoiceGenerator<S, Sample>
    where
        Sample: SIMDSample<S>,
        SIMDSampleMono<S>: Mul<Sample, Output = Sample>,
        Gen: SIMDVoiceGenerator<S, Sample>,
    {
        let modified_params = SIMDVoiceEnvelope::<S>::get_modified_envelope(
            *self.volume_envelope_params.clone(),
            control.envelope,
            self.stream_params.sample_rate as f32,
        );

        let volume_envelope = SIMDVoiceEnvelope::new(
            *self.volume_envelope_params.clone(),
            modified_params,
            self.stream_params.sample_rate as f32,
        );

        let amp = VoiceCombineSIMD::mult(volume_envelope, gen);
        amp
    }

    fn convert_to_voice<Gen>(&self, gen: Gen) -> Box<dyn Voice>
    where
        Gen: 'static + SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
    {
        let flattened = SIMDStereoVoice::new(gen);
        let base = VoiceBase::new(self.vel, flattened);

        Box::new(base)
    }
}

impl<S: 'static + Sync + Send + Simd> VoiceSpawner for SampledVoiceSpawner<S> {
    fn spawn_voice(&self, control: &VoiceControlData) -> Box<dyn Voice> {
        let gen = self.get_sampler(control);

        let gen = self.apply_velocity(gen);
        let gen = self.apply_pan(gen);
        let gen = self.apply_envelope(gen, control);

        if let Some(filter) = &self.filter {
            let gen = SIMDStereoVoiceCutoff::new(gen, filter);
            self.convert_to_voice(gen)
        } else {
            self.convert_to_voice(gen)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SoundfontInitOptions {
    pub linear_release: bool,
    pub use_effects: bool,
}

impl Default for SoundfontInitOptions {
    fn default() -> Self {
        Self {
            linear_release: false,
            use_effects: true,
        }
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
        release: env.ampeg_release.max(0.02),
    }
}

#[derive(Debug, Error)]
pub enum LoadSfzError {
    #[error("IO Error")]
    IOError(#[from] io::Error),

    #[error("Error loading samples")]
    AudioLoadError(#[from] AudioLoadError),

    #[error("Error parsing the SFZ: {0}")]
    SfzParseError(#[from] SfzParseError),
}

impl SampleSoundfont {
    pub fn new(
        sfz_path: impl Into<PathBuf>,
        stream_params: AudioStreamParams,
        options: SoundfontInitOptions,
    ) -> Result<Self, LoadSfzError> {
        if stream_params.channels == ChannelCount::Mono {
            panic!("Mono output is currently not supported");
        }

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
            let envelope_descriptor = envelope_descriptor_from_region_params(region);
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
        let mut spawner_params_list = Vec::<Option<Arc<SampleVoiceSpawnerParams>>>::new();
        for _ in 0..(128 * 128) {
            spawner_params_list.push(None);
        }

        // Write region params
        for region in regions {
            let params = sample_cache_from_region_params(&region);
            let envelope = envelope_descriptor_from_region_params(&region);

            for key in region.keyrange.clone() {
                for vel in region.velrange.clone() {
                    let index = key_vel_to_index(key, vel);
                    let speed_mult =
                        get_speed_mult_from_keys(key, region.pitch_keycenter.unwrap_or(key));

                    let envelope_params = unique_envelope_params
                        .iter()
                        .find(|e| e.0 == envelope)
                        .unwrap()
                        .1
                        .clone();

                    let mut cutoff = None;
                    if let Some(mut cutoff_t) = region.cutoff {
                        if cutoff_t >= 1.0 {
                            let cents = vel as f32 / 127.0 * region.fil_veltrack as f32
                                + (key as f32 - region.fil_keycenter as f32)
                                    * region.fil_keytrack as f32;
                            cutoff_t *= 2.0f32.powf(cents / 1200.0);
                            cutoff = Some(
                                cutoff_t.clamp(1.0, stream_params.sample_rate as f32 / 2.0 - 100.0),
                            );
                        }
                    }

                    let pan = ((region.pan as f32 / 100.0) + 1.0) / 2.0;
                    let volume = 10f32.powf(region.volume as f32 / 20.0);

                    let spawner_params = Arc::new(SampleVoiceSpawnerParams {
                        pan,
                        volume,
                        envelope: envelope_params,
                        speed_mult,
                        cutoff,
                        filter_type: region.filter_type,
                        sample: samples[&params].clone(),
                    });

                    spawner_params_list[index] = Some(spawner_params.clone());
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
    fn stream_params(&self) -> &'_ AudioStreamParams {
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
                    vec![Box::new(SampledVoiceSpawner::<S>::new(
                        spawner_params,
                        vel,
                        sf.stream_params,
                    ))]
                } else {
                    vec![]
                }
            }
        );

        get_runtime_select(key, vel, self)
    }

    fn get_release_voice_spawners_at(&self, _key: u8, _vel: u8) -> Vec<Box<dyn VoiceSpawner>> {
        vec![]
    }
}
