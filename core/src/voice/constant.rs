use simdeez::Simd;

use crate::voice::VoiceControlData;

use super::{SIMDSampleMono, SIMDVoiceGenerator, VoiceGeneratorBase};

pub struct SIMDConstant<S: Simd> {
    values: S::Vf32,
}

impl<S: Simd> SIMDConstant<S> {
    pub fn new(value: f32) -> SIMDConstant<S> {
        unsafe {
            SIMDConstant {
                values: S::set1_ps(value),
            }
        }
    }
}

impl<S: Simd> VoiceGeneratorBase for SIMDConstant<S> {
    #[inline(always)]
    fn ended(&self) -> bool {
        false
    }

    #[inline(always)]
    fn signal_release(&mut self) {}

    #[inline(always)]
    fn process_controls(&mut self, _control: &VoiceControlData) {}
}

impl<S: Simd> SIMDVoiceGenerator<S, SIMDSampleMono<S>> for SIMDConstant<S> {
    #[inline(always)]
    fn next_sample(&mut self) -> SIMDSampleMono<S> {
        SIMDSampleMono(self.values)
    }
}
