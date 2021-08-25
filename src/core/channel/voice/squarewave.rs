use std::marker::PhantomData;

use simdeez::Simd;

use super::{SIMDSampleMono, SIMDVoiceGenerator, VoiceGeneratorBase};

pub struct SIMDSquareWaveGenerator<S: Simd> {
    base_step: f32,

    phase: f32,

    _s: PhantomData<S>,
}

impl<S: Simd> SIMDSquareWaveGenerator<S> {
    pub fn new(base_freq: f32, sample_rate: u32) -> Self {
        let freq = base_freq / sample_rate as f32;

        Self {
            base_step: freq,
            phase: 0.0,
            _s: PhantomData,
        }
    }

    fn next_phase(&mut self, step: f32) -> f32 {
        self.phase += step;
        self.phase %= 1.0;
        self.phase
    }
}

impl<S: Simd> VoiceGeneratorBase for SIMDSquareWaveGenerator<S> {
    fn ended(&self) -> bool {
        false
    }

    fn signal_release(&mut self) {}
}

impl<S: Simd> SIMDVoiceGenerator<S, SIMDSampleMono<S>> for SIMDSquareWaveGenerator<S> {
    fn next_sample(&mut self) -> SIMDSampleMono<S> {
        let mut values = unsafe { S::set1_ps(0.0) };
        for i in 0..S::VF32_WIDTH {
            let phase = self.next_phase(self.base_step);
            let val = if phase > 0.5 { 1.0 } else { -1.0 };
            values[i] = val;
        }

        SIMDSampleMono(values)
    }
}
