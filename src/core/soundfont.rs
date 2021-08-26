use std::marker::PhantomData;

use simdeez::Simd;

use super::voice::{
    EnvelopeParameters, SIMDConstant, SIMDSquareWaveGenerator, SIMDStereoVoice, SIMDVoiceEnvelope,
    SIMDVoiceMonoToStereo, Voice, VoiceBase, VoiceCombineSIMD,
};
use crate::{core::voice::EnvelopeDescriptor, helpers::FREQS, AudioStreamParams};

pub trait VoiceSpawner: Sync + Send {
    fn spawn_voice(&self) -> Box<dyn Voice>;
}

pub trait SoundfontBase: Sync + Send {
    fn stream_params<'a>(&'a self) -> &'a AudioStreamParams;

    fn get_attack_voice_spawners_at(&self, key: u8, vel: u8) -> Vec<Box<dyn VoiceSpawner>>;
    fn get_release_voice_spawners_at(&self, key: u8) -> Vec<Box<dyn VoiceSpawner>>;
}

// pub struct SineVoice {
//     freq: f64,

//     amp: f32,
//     phase: f64,
// }

// impl SineVoice {
//     pub fn spawn(key: u8, vel: u8, sample_rate: u32) -> Self {
//         let freq = (FREQS[key as usize] as f64 / sample_rate as f64) * std::f64::consts::PI;
//         let amp = 1.04f32.powf(vel as f32 - 127.0);

//         Self {
//             freq,
//             amp,
//             phase: 0.0,
//         }
//     }
// }

// impl Voice for SineVoice {
//     fn is_ended(&self) -> bool {
//         self.amp == 0.0
//     }

//     fn is_releasing(&self) -> bool {
//         self.is_ended()
//     }

//     fn signal_release(&mut self) {
//         self.amp = 0.0;
//     }

//     fn render_to(&mut self, out: &mut [f32]) {
//         for i in 0..out.len() {
//             let sample = self.phase.cos() as f32;
//             let sample = if sample > 0.0 { 1.0 } else { -1.0 };
//             let sample = self.amp * sample;
//             self.phase += self.freq;
//             out[i] += sample;
//         }
//     }
// }

struct SquareVoiceSpawner<S: 'static + Simd + Send + Sync> {
    sample_rate: u32,
    base_freq: f32,
    amp: f32,
    volume_envelope_params: EnvelopeParameters,
    _s: PhantomData<S>,
}

impl<S: Simd + Send + Sync> SquareVoiceSpawner<S> {
    pub fn new(
        key: u8,
        vel: u8,
        sample_rate: u32,
        volume_envelope_params: EnvelopeParameters,
    ) -> Self {
        let base_freq = FREQS[key as usize];
        let amp = 1.04f32.powf(vel as f32 - 127.0);

        Self {
            sample_rate,
            base_freq,
            amp,
            volume_envelope_params,
            _s: PhantomData,
        }
    }
}

impl<S: 'static + Sync + Send + Simd> VoiceSpawner for SquareVoiceSpawner<S> {
    fn spawn_voice(&self) -> Box<dyn Voice> {
        let pitch_fac = SIMDConstant::<S>::new(self.base_freq / self.sample_rate as f32);
        let square = SIMDSquareWaveGenerator::new(pitch_fac);
        let square = SIMDVoiceMonoToStereo::new(square);
        let amp = SIMDConstant::<S>::new(self.amp);
        let volume_envelope = SIMDVoiceEnvelope::new(self.volume_envelope_params);

        let modulated = VoiceCombineSIMD::mult(amp, square);
        let modulated = VoiceCombineSIMD::mult(volume_envelope, modulated);

        let flattened = SIMDStereoVoice::new(modulated);
        let base = VoiceBase::new(flattened);

        Box::new(base)
    }
}

pub struct SquareSoundfont {
    stream_params: AudioStreamParams,
}

impl SquareSoundfont {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            stream_params: AudioStreamParams::new(sample_rate, channels),
        }
    }
}

impl SoundfontBase for SquareSoundfont {
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
            fn get(key: u8, vel: u8, sample_rate: u32) -> Vec<Box<dyn VoiceSpawner>> {
                let envelope_descriptor = EnvelopeDescriptor {
                    start_percent: 0.0,
                    delay: 0.0,
                    attack: 0.0,
                    hold: 0.0,
                    decay: 0.1,
                    sustain_percent: 1.0,
                    release: 0.2,
                };

                vec![Box::new(SquareVoiceSpawner::<S>::new(
                    key,
                    vel,
                    sample_rate,
                    envelope_descriptor.to_envelope_params(sample_rate),
                ))]
            }
        );

        get_runtime_select(key, vel, self.stream_params.sample_rate)
    }

    fn get_release_voice_spawners_at(&self, _key: u8) -> Vec<Box<dyn VoiceSpawner>> {
        vec![]
    }
}
