use std::marker::PhantomData;

use simdeez::prelude::*;

use crate::voice::{ReleaseType, VoiceControlData};

use super::{
    SIMDSample, SIMDSampleMono, SIMDSampleStereo, SIMDVoiceGenerator, VoiceGeneratorBase,
    VoiceSampleGenerator,
};

pub struct SIMDStereoVoice<S: Simd, T: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>> {
    generator: T,
    remainder: SIMDSampleStereo<S>,
    remainder_pos: usize,
    _s: PhantomData<S>,
}

impl<S: Simd, T: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>> SIMDStereoVoice<S, T> {
    pub fn new(generator: T) -> SIMDStereoVoice<S, T> {
        SIMDStereoVoice {
            generator,
            remainder: SIMDSampleStereo::<S>::zero(),
            remainder_pos: S::Vf32::WIDTH,
            _s: PhantomData,
        }
    }
}

impl<S, T> VoiceGeneratorBase for SIMDStereoVoice<S, T>
where
    S: Simd,
    T: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
{
    #[inline(always)]
    fn ended(&self) -> bool {
        self.generator.ended()
    }

    #[inline(always)]
    fn signal_release(&mut self, rel_type: ReleaseType) {
        self.generator.signal_release(rel_type)
    }

    #[inline(always)]
    fn process_controls(&mut self, control: &VoiceControlData) {
        self.generator.process_controls(control)
    }
}

impl<S, T> VoiceSampleGenerator for SIMDStereoVoice<S, T>
where
    S: Simd,
    T: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
{
    fn render_to(&mut self, buffer: &mut [f32]) {
        simd_invoke!(S, {
            for chunk in buffer.chunks_exact_mut(2) {
                if self.remainder_pos == S::Vf32::WIDTH {
                    self.remainder = self.generator.next_sample();
                    self.remainder_pos = 0;
                }

                unsafe {
                    // using get_unchecked here is safe because we check the bounds above
                    // however the compiler doesn't seem to detect it otherwise.
                    chunk[0] += self.remainder.0.get_unchecked(self.remainder_pos);
                    chunk[1] += self.remainder.1.get_unchecked(self.remainder_pos);
                }

                self.remainder_pos += 1;
            }
        })
    }
}

pub struct SIMDMonoVoice<S: Simd, T: SIMDVoiceGenerator<S, SIMDSampleMono<S>>> {
    generator: T,
    remainder: SIMDSampleMono<S>,
    remainder_pos: usize,
    _s: PhantomData<S>,
}

impl<S: Simd, T: SIMDVoiceGenerator<S, SIMDSampleMono<S>>> SIMDMonoVoice<S, T> {
    pub fn new(generator: T) -> SIMDMonoVoice<S, T> {
        SIMDMonoVoice {
            generator,
            remainder: SIMDSampleMono::<S>::zero(),
            remainder_pos: S::Vf32::WIDTH,
            _s: PhantomData,
        }
    }
}

impl<S, T> VoiceGeneratorBase for SIMDMonoVoice<S, T>
where
    S: Simd,
    T: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
{
    #[inline(always)]
    fn ended(&self) -> bool {
        self.generator.ended()
    }

    #[inline(always)]
    fn signal_release(&mut self, rel_type: ReleaseType) {
        self.generator.signal_release(rel_type)
    }

    #[inline(always)]
    fn process_controls(&mut self, control: &VoiceControlData) {
        self.generator.process_controls(control)
    }
}

impl<S, T> VoiceSampleGenerator for SIMDMonoVoice<S, T>
where
    S: Simd,
    T: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
{
    fn render_to(&mut self, buffer: &mut [f32]) {
        simd_invoke!(S, {
            let mut i = 0;
            while i < buffer.len() {
                if self.remainder_pos == S::Vf32::WIDTH {
                    self.remainder = self.generator.next_sample();
                    self.remainder_pos = 0;
                }

                buffer[i] += self.remainder.0[self.remainder_pos];
                i += 1;

                self.remainder_pos += 1;
            }
        })
    }
}
