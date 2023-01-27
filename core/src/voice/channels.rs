use std::marker::PhantomData;

use simdeez::prelude::*;

use crate::voice::VoiceControlData;

use super::{
    ReleaseType, SIMDSampleMono, SIMDSampleStereo, SIMDVoiceGenerator, VoiceGeneratorBase,
};

pub struct SIMDVoiceMonoToStereo<S, G>
where
    S: Simd,
    G: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
{
    generator: G,

    _s: PhantomData<S>,
}

impl<S: Simd, G: SIMDVoiceGenerator<S, SIMDSampleMono<S>>> SIMDVoiceMonoToStereo<S, G> {
    pub fn new(generator: G) -> SIMDVoiceMonoToStereo<S, G> {
        SIMDVoiceMonoToStereo {
            generator,
            _s: PhantomData,
        }
    }
}

impl<S, G> VoiceGeneratorBase for SIMDVoiceMonoToStereo<S, G>
where
    S: Simd,
    G: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
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

impl<S, G> SIMDVoiceGenerator<S, SIMDSampleStereo<S>> for SIMDVoiceMonoToStereo<S, G>
where
    S: Simd,
    G: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
{
    #[inline(always)]
    fn next_sample(&mut self) -> SIMDSampleStereo<S> {
        simd_invoke!(S, {
            let sample = self.generator.next_sample();
            SIMDSampleStereo(sample.0, sample.0)
        })
    }
}
