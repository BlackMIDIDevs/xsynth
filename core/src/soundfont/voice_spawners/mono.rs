use std::{marker::PhantomData, ops::Mul, sync::Arc};

use simdeez::Simd;

use crate::{
    effects::BiQuadFilter,
    voice::{
        gain_ramp_frames, modulate_envelope, BufferSampler, CutoffModulation, GainModulation,
        SIMDMonoVoiceCutoff, SIMDSample, SIMDSampleGrabber, SIMDSampleMono, SIMDVoiceGainMono,
        SIMDVoiceGenerator, SIMDVoicePitch,
    },
    AudioStreamParams,
};
use crate::{
    voice::VoiceControlData,
    voice::{
        BufferSamplers, EnvelopeParameters, SIMDLinearSampleGrabber, SIMDMonoVoice,
        SIMDMonoVoiceSampler, SIMDNearestSampleGrabber, SIMDVoiceEnvelope, SampleReader,
        SampleReaderLoop, SampleReaderLoopSustain, SampleReaderNoLoop, Voice, VoiceBase,
        VoiceCombineSIMD,
    },
};

use xsynth_soundfonts::{
    modulation::{CcRange, ControllerMask, RegionModulation},
    FilterType, LoopMode,
};

use crate::soundfont::{
    EnvelopeOptions, Interpolator, LoopParams, SampleVoiceSpawnerParams, VoiceSpawner,
};

use crate::voice::EnvelopeDescriptor;

pub struct MonoSampledVoiceSpawner<S: 'static + Simd + Send + Sync> {
    speed_mult: f32,
    filter: Option<BiQuadFilter>,
    filter_type: FilterType,
    cutoff: f32,
    resonance: f32,
    loop_params: LoopParams,
    amp: f32,
    volume_envelope_params: Arc<EnvelopeParameters>,
    envelope_descriptor: EnvelopeDescriptor,
    envelope_options: EnvelopeOptions,
    modulation: Option<Arc<RegionModulation>>,
    cc_conditions: Arc<[CcRange]>,
    used_controllers: ControllerMask,
    effect_sends: (Option<f32>, Option<f32>),
    samples: Arc<[Arc<[f32]>]>,
    interpolator: Interpolator,
    exclusive_class: Option<u8>,
    key: u8,
    vel: u8,
    stream_params: AudioStreamParams,
    _s: PhantomData<S>,
}

impl<S: Simd + Send + Sync> MonoSampledVoiceSpawner<S> {
    pub fn new(
        params: &SampleVoiceSpawnerParams,
        key: u8,
        vel: u8,
        stream_params: AudioStreamParams,
    ) -> Self {
        let amp = params.volume;

        let filter = params.cutoff.map(|cutoff| {
            BiQuadFilter::new(
                params.filter_type,
                cutoff,
                stream_params.sample_rate as f32,
                Some(params.resonance),
            )
        });

        Self {
            speed_mult: params.speed_mult,
            filter,
            filter_type: params.filter_type,
            cutoff: params.cutoff.unwrap_or(0.0),
            resonance: params.resonance,
            loop_params: params.loop_params.clone(),
            amp,
            volume_envelope_params: params.envelope.clone(),
            envelope_descriptor: params.envelope_descriptor,
            envelope_options: params.envelope_options,
            modulation: params.modulation.clone(),
            cc_conditions: params.cc_conditions.clone(),
            used_controllers: params.used_controllers,
            effect_sends: (params.reverb_send, params.chorus_send),
            samples: params.sample.clone(),
            interpolator: params.interpolator,
            exclusive_class: params.exclusive_class,
            key,
            vel,
            stream_params,
            _s: PhantomData,
        }
    }

    fn begin_voice(&self, control: &VoiceControlData) -> Box<dyn Voice> {
        // Currently there's only the f32 buffer samples, more could be added in the future.
        #[allow(clippy::redundant_closure)]
        self.make_sample_reader(control, |s| BufferSamplers::new_f32(s))
    }

    fn make_sample_reader<BS: 'static + BufferSampler>(
        &self,
        control: &VoiceControlData,
        make_bs: impl Fn(Arc<[f32]>) -> BS,
    ) -> Box<dyn Voice> {
        match self.loop_params.mode {
            LoopMode::LoopContinuous => self.make_sample_grabber(control, move |s| {
                SampleReaderLoop::new(make_bs(s), self.loop_params.clone())
            }),
            LoopMode::LoopSustain => self.make_sample_grabber(control, move |s| {
                SampleReaderLoopSustain::new(make_bs(s), self.loop_params.clone())
            }),
            LoopMode::NoLoop | LoopMode::OneShot => self.make_sample_grabber(control, move |s| {
                SampleReaderNoLoop::new(make_bs(s), self.loop_params.clone())
            }),
        }
    }

    fn make_sample_grabber<SR: 'static + SampleReader>(
        &self,
        control: &VoiceControlData,
        make_bs: impl Fn(Arc<[f32]>) -> SR,
    ) -> Box<dyn Voice> {
        match self.interpolator {
            Interpolator::Nearest => {
                self.generate_sampler(control, |s| SIMDNearestSampleGrabber::new(make_bs(s)))
            }
            Interpolator::Linear => {
                self.generate_sampler(control, |s| SIMDLinearSampleGrabber::new(make_bs(s)))
            }
        }
    }

    fn generate_sampler<SG: 'static + SIMDSampleGrabber<S>>(
        &self,
        control: &VoiceControlData,
        make_sampler: impl Fn(Arc<[f32]>) -> SG,
    ) -> Box<dyn Voice> {
        let sample = make_sampler(self.samples[0].clone());

        let pitch_fac = self.create_pitch_fac(control);

        let sampler = SIMDMonoVoiceSampler::new(sample, pitch_fac);
        self.apply_voice_params(sampler, control)
    }

    fn apply_gain<Gen, Sample>(
        &self,
        gen: Gen,
        control: &VoiceControlData,
    ) -> impl SIMDVoiceGenerator<S, Sample>
    where
        Sample: SIMDSample<S>,
        SIMDSampleMono<S>: Mul<Sample, Output = Sample>,
        Gen: SIMDVoiceGenerator<S, Sample>,
    {
        let gains = SIMDVoiceGainMono::<S>::new(
            self.amp * control.note_gain,
            self.modulation.clone().map(GainModulation::new),
            control,
            gain_ramp_frames::<S>(self.stream_params.sample_rate),
        );
        let gen = VoiceCombineSIMD::mult(gains, gen);
        gen
    }

    fn create_pitch_fac(
        &self,
        control: &VoiceControlData,
    ) -> impl SIMDVoiceGenerator<S, SIMDSampleMono<S>> {
        let glide = control.portamento.map(|p| {
            (
                (p.from_key as f32 - self.key as f32) * 100.0,
                p.time * self.stream_params.sample_rate as f32,
            )
        });
        SIMDVoicePitch::<S>::new(self.speed_mult, self.modulation.clone(), control, glide)
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
        let base_params = match &self.modulation {
            Some(m) if m.has_note_on_envelope() => {
                modulate_envelope(self.envelope_descriptor, m, &control.controllers)
                    .to_envelope_params(self.stream_params.sample_rate, self.envelope_options)
            }
            _ => *self.volume_envelope_params,
        };

        let modified_params = SIMDVoiceEnvelope::<S>::get_modified_envelope(
            base_params,
            control.envelope,
            self.stream_params.sample_rate as f32,
        );

        let allow_release = self.loop_params.mode != LoopMode::OneShot;

        let volume_envelope = SIMDVoiceEnvelope::new(
            base_params,
            modified_params,
            allow_release,
            self.stream_params.sample_rate as f32,
        );

        let amp = VoiceCombineSIMD::mult(volume_envelope, gen);
        amp
    }

    fn convert_to_voice<Gen>(&self, gen: Gen) -> Box<dyn Voice>
    where
        Gen: 'static + SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
    {
        let flattened = SIMDMonoVoice::new(gen);
        let base = VoiceBase::new(self.vel, self.exclusive_class(), flattened);

        Box::new(base)
    }

    fn apply_voice_params<Gen>(&self, gen: Gen, control: &VoiceControlData) -> Box<dyn Voice>
    where
        Gen: 'static + SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
    {
        let gen = self.apply_gain(gen, control);
        let gen = self.apply_envelope(gen, control);

        self.apply_cutoff_effect(gen, control)
    }

    fn apply_cutoff_effect(
        &self,
        gen: impl 'static + SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
        control: &VoiceControlData,
    ) -> Box<dyn Voice> {
        if let Some(filter) = &self.filter {
            let modulation = self.modulation.clone().and_then(|m| {
                CutoffModulation::new(
                    m,
                    self.filter_type,
                    self.cutoff,
                    self.resonance,
                    self.stream_params.sample_rate as f32,
                )
            });
            let gen = SIMDMonoVoiceCutoff::new_modulated(gen, filter, modulation, control);
            self.convert_to_voice(gen)
        } else {
            self.convert_to_voice(gen)
        }
    }
}

impl<S: 'static + Sync + Send + Simd> VoiceSpawner for MonoSampledVoiceSpawner<S> {
    fn spawn_voice(&self, control: &VoiceControlData) -> Box<dyn Voice> {
        self.begin_voice(control)
    }

    fn exclusive_class(&self) -> Option<u8> {
        self.exclusive_class
    }

    fn can_spawn(&self, control: &VoiceControlData) -> bool {
        self.cc_conditions
            .iter()
            .all(|range| range.contains(control.controllers.cc[range.cc as usize]))
    }

    fn used_controllers(&self) -> ControllerMask {
        self.used_controllers
    }

    fn effect_sends(&self) -> (Option<f32>, Option<f32>) {
        self.effect_sends
    }
}
