use simdeez::prelude::*;

use crate::voice::{ReleaseType, VoiceControlData};

use super::{SIMDSampleMono, SIMDSampleStereo, SIMDVoiceGenerator, VoiceGeneratorBase};

pub struct SIMDConstant<S: Simd> {
    values: S::Vf32,
}

impl<S: Simd> SIMDConstant<S> {
    pub fn new(value: f32) -> SIMDConstant<S> {
        simd_invoke!(S, {
            SIMDConstant {
                values: S::Vf32::set1(value),
            }
        })
    }
}

impl<S: Simd> VoiceGeneratorBase for SIMDConstant<S> {
    #[inline(always)]
    fn ended(&self) -> bool {
        false
    }

    #[inline(always)]
    fn signal_release(&mut self, _rel_type: ReleaseType) {}

    #[inline(always)]
    fn process_controls(&mut self, _control: &VoiceControlData) {}
}

impl<S: Simd> SIMDVoiceGenerator<S, SIMDSampleMono<S>> for SIMDConstant<S> {
    #[inline(always)]
    fn next_sample(&mut self) -> SIMDSampleMono<S> {
        SIMDSampleMono(self.values)
    }
}

pub struct SIMDConstantStereo<S: Simd> {
    values_left: S::Vf32,
    values_right: S::Vf32,
}

impl<S: Simd> SIMDConstantStereo<S> {
    pub fn new(value_left: f32, value_right: f32) -> SIMDConstantStereo<S> {
        simd_invoke!(S, {
            SIMDConstantStereo {
                values_left: S::Vf32::set1(value_left),
                values_right: S::Vf32::set1(value_right),
            }
        })
    }
}

impl<S: Simd> VoiceGeneratorBase for SIMDConstantStereo<S> {
    #[inline(always)]
    fn ended(&self) -> bool {
        false
    }

    #[inline(always)]
    fn signal_release(&mut self, _rel_type: ReleaseType) {}

    #[inline(always)]
    fn process_controls(&mut self, _control: &VoiceControlData) {}
}

impl<S: Simd> SIMDVoiceGenerator<S, SIMDSampleStereo<S>> for SIMDConstantStereo<S> {
    #[inline(always)]
    fn next_sample(&mut self) -> SIMDSampleStereo<S> {
        SIMDSampleStereo(self.values_left, self.values_right)
    }
}
