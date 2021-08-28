use simdeez::Simd;

use crate::core::VoiceControlData;

use super::{SIMDSampleMono, SIMDVoiceGenerator, VoiceGeneratorBase};

pub struct SIMDVoiceControl<S: Simd> {
    values: S::Vf32,
    update: fn(&VoiceControlData) -> f32,
}

impl<S: Simd> SIMDVoiceControl<S> {
    pub fn new(control: &VoiceControlData, update: fn(&VoiceControlData) -> f32) -> SIMDVoiceControl<S> {
        unsafe {
            SIMDVoiceControl {
                values: S::set1_ps((update)(control)),
                update,
            }
        }
    }
}

impl<S: Simd> VoiceGeneratorBase for SIMDVoiceControl<S> {
    fn ended(&self) -> bool {
        false
    }

    fn signal_release(&mut self) {}

    fn process_controls(&mut self, control: &VoiceControlData) {
        unsafe {
            self.values = S::set1_ps((self.update)(control));
        }
    }
}

impl<S: Simd> SIMDVoiceGenerator<S, SIMDSampleMono<S>> for SIMDVoiceControl<S> {
    fn next_sample(&mut self) -> SIMDSampleMono<S> {
        SIMDSampleMono(self.values)
    }
}
