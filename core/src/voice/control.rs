use simdeez::prelude::*;

use crate::voice::{ReleaseType, VoiceControlData};

use super::{SIMDSampleMono, SIMDVoiceGenerator, VoiceGeneratorBase};

pub struct SIMDVoiceControl<S: Simd> {
    values: S::Vf32,
    update: fn(&VoiceControlData) -> f32,
}

impl<S: Simd> SIMDVoiceControl<S> {
    pub fn new(
        control: &VoiceControlData,
        update: fn(&VoiceControlData) -> f32,
    ) -> SIMDVoiceControl<S> {
        simd_invoke!(S, {
            SIMDVoiceControl {
                values: S::Vf32::set1((update)(control)),
                update,
            }
        })
    }
}

impl<S: Simd> VoiceGeneratorBase for SIMDVoiceControl<S> {
    #[inline(always)]
    fn ended(&self) -> bool {
        false
    }

    #[inline(always)]
    fn signal_release(&mut self, _rel_type: ReleaseType) {}

    #[inline(always)]
    fn process_controls(&mut self, control: &VoiceControlData) {
        simd_invoke!(S, {
            self.values = S::Vf32::set1((self.update)(control));
        })
    }
}

impl<S: Simd> SIMDVoiceGenerator<S, SIMDSampleMono<S>> for SIMDVoiceControl<S> {
    #[inline(always)]
    fn next_sample(&mut self) -> SIMDSampleMono<S> {
        SIMDSampleMono(self.values)
    }
}
