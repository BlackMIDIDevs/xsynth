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
use soundfonts::{sfz::RegionParams, CutoffPassCount};
use thiserror::Error;

use self::audio::{load_audio_file, AudioLoadError};

use super::{
    voice::VoiceControlData,
    voice::{
        BufferSamplers, EnvelopeParameters, EnvelopePart, EnvelopeStage, SIMDConstant,
        SIMDNearestSampleGrabber, SIMDStereoVoice, SIMDStereoVoiceSampler, SIMDVoiceControl,
        SIMDVoiceEnvelope, SampleReader, Voice, VoiceBase, VoiceCombineSIMD,
    },
};
use crate::{
    effects::{Highpass, Lowpass, MultiPassCutoff},
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
    speed_mult: f32,
    sample_rate: f32,
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
    cutoff: Option<f32>,
    filter_type: FilterType,
    amp: f32,
    volume_envelope_params: Arc<EnvelopeParameters>,
    samples: Arc<[Arc<[f32]>]>,
    sample_rate: f32,
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
        let amp = (vel as f32 / 127.0).powi(2);

        Self {
            speed_mult: params.speed_mult,
            cutoff: params.cutoff,
            filter_type: params.filter_type,
            amp,
            volume_envelope_params: params.envelope.clone(),
            samples: params.sample.clone(),
            sample_rate: params.sample_rate,
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
        let mut params = *self.volume_envelope_params.clone();

        fn calculate_curve(value: u8, duration: f32) -> f32 {
            match value {
                0..=64 => (value as f32 / 64.0).powi(5) * duration,
                65..=128 => duration + ((value as f32 - 64.0) / 64.0).powi(3) * 15.0,
                _ => duration,
            }
        }

        if let Some(attack) = control.attack {
            let duration = params.get_stage_duration::<S>(EnvelopeStage::Attack) as f32
                / self.stream_params.sample_rate as f32;
            params.modify_stage_data::<S>(
                1,
                EnvelopePart::lerp(
                    1.0,
                    (calculate_curve(attack, duration) * self.stream_params.sample_rate as f32)
                        as u32,
                ),
            );
        }
        if let Some(release) = control.release {
            let duration = params.get_stage_duration::<S>(EnvelopeStage::Release) as f32
                / self.stream_params.sample_rate as f32;
            params.modify_stage_data::<S>(
                5,
                EnvelopePart::lerp_to_zero_curve(
                    (calculate_curve(release, duration).max(0.02)
                        * self.stream_params.sample_rate as f32) as u32,
                ),
            );
        }

        let volume_envelope = SIMDVoiceEnvelope::new(params);
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
        let gen = self.apply_envelope(gen, control);

        if let Some(cutoff) = self.cutoff {
            match self.filter_type {
                FilterType::LowPass {
                    passes: CutoffPassCount::One,
                } => {
                    let gen = SIMDStereoVoiceCutoff::new(
                        gen,
                        MultiPassCutoff::<Lowpass, 1>::new(cutoff, self.sample_rate),
                    );
                    self.convert_to_voice(gen)
                }
                FilterType::LowPass {
                    passes: CutoffPassCount::Two,
                } => {
                    let gen = SIMDStereoVoiceCutoff::new(
                        gen,
                        MultiPassCutoff::<Lowpass, 2>::new(cutoff, self.sample_rate),
                    );
                    self.convert_to_voice(gen)
                }
                FilterType::LowPass {
                    passes: CutoffPassCount::Four,
                } => {
                    let gen = SIMDStereoVoiceCutoff::new(
                        gen,
                        MultiPassCutoff::<Lowpass, 4>::new(cutoff, self.sample_rate),
                    );
                    self.convert_to_voice(gen)
                }
                FilterType::LowPass {
                    passes: CutoffPassCount::Six,
                } => {
                    let gen = SIMDStereoVoiceCutoff::new(
                        gen,
                        MultiPassCutoff::<Lowpass, 6>::new(cutoff, self.sample_rate),
                    );
                    self.convert_to_voice(gen)
                }
                FilterType::HighPass {
                    passes: CutoffPassCount::One,
                } => {
                    let gen = SIMDStereoVoiceCutoff::new(
                        gen,
                        MultiPassCutoff::<Highpass, 1>::new(cutoff, self.sample_rate),
                    );
                    self.convert_to_voice(gen)
                }
                FilterType::HighPass {
                    passes: CutoffPassCount::Two,
                } => {
                    let gen = SIMDStereoVoiceCutoff::new(
                        gen,
                        MultiPassCutoff::<Highpass, 2>::new(cutoff, self.sample_rate),
                    );
                    self.convert_to_voice(gen)
                }
                FilterType::HighPass {
                    passes: CutoffPassCount::Four,
                } => {
                    let gen = SIMDStereoVoiceCutoff::new(
                        gen,
                        MultiPassCutoff::<Highpass, 4>::new(cutoff, self.sample_rate),
                    );
                    self.convert_to_voice(gen)
                }
                FilterType::HighPass {
                    passes: CutoffPassCount::Six,
                } => {
                    let gen = SIMDStereoVoiceCutoff::new(
                        gen,
                        MultiPassCutoff::<Highpass, 6>::new(cutoff, self.sample_rate),
                    );
                    self.convert_to_voice(gen)
                }
            }
        } else {
            self.convert_to_voice(gen)
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
}

impl SampleSoundfont {
    pub fn new(
        sfz_path: impl Into<PathBuf>,
        stream_params: AudioStreamParams,
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
                    if let Some(cutoff_t) = region.cutoff {
                        if cutoff_t < 1.0 {
                            cutoff = None
                        } else {
                            let mut cutoff_t =
                                cutoff_t.clamp(1.0, stream_params.sample_rate as f32 / 2.0);
                            let cents = vel as f32 / 127.0 * region.fil_veltrack as f32
                                + (key - region.fil_keycenter) as f32 * region.fil_keytrack as f32;
                            cutoff_t *= 2.0f32.powf(cents / 1200.0);
                            cutoff = Some(cutoff_t);
                        }
                    }

                    let spawner_params = Arc::new(SampleVoiceSpawnerParams {
                        envelope: envelope_params,
                        speed_mult,
                        cutoff,
                        filter_type: region.filter_type,
                        sample: samples[&params].clone(),
                        sample_rate: stream_params.sample_rate as f32,
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
