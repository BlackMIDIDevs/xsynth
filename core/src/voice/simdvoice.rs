use std::marker::PhantomData;

use simdeez::Simd;

use crate::voice::VoiceControlData;

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
            remainder_pos: S::VF32_WIDTH,
            _s: PhantomData,
        }
    }
}

impl<S, T> VoiceGeneratorBase for SIMDStereoVoice<S, T>
where
    S: Simd,
    T: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
{
    fn ended(&self) -> bool {
        self.generator.ended()
    }

    fn signal_release(&mut self) {
        self.generator.signal_release()
    }

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
        let mut i = 0;
        while i < buffer.len() {
            if self.remainder_pos == S::VF32_WIDTH {
                self.remainder = self.generator.next_sample();
                self.remainder_pos = 0;
            }

            buffer[i] += self.remainder.0[self.remainder_pos];
            i += 1;
            buffer[i] += self.remainder.1[self.remainder_pos];
            i += 1;

            self.remainder_pos += 1;
        }
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
            remainder_pos: S::VF32_WIDTH,
            _s: PhantomData,
        }
    }
}

impl<S, T> VoiceGeneratorBase for SIMDMonoVoice<S, T>
where
    S: Simd,
    T: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
{
    fn ended(&self) -> bool {
        self.generator.ended()
    }

    fn signal_release(&mut self) {
        self.generator.signal_release()
    }

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
        let mut i = 0;
        while i < buffer.len() {
            if self.remainder_pos == S::VF32_WIDTH {
                self.remainder = self.generator.next_sample();
                self.remainder_pos = 0;
            }

            buffer[i] += self.remainder.0[self.remainder_pos];
            i += 1;

            self.remainder_pos += 1;
        }
    }
}
