use std::marker::PhantomData;

use simdeez::Simd;

use super::{SIMDSampleMono, SIMDSampleStereo, SIMDVoiceGenerator, VoiceGeneratorBase};

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
            generator: generator,
            _s: PhantomData,
        }
    }
}

impl<S, G> VoiceGeneratorBase for SIMDVoiceMonoToStereo<S, G>
where
    S: Simd,
    G: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
{
    fn ended(&self) -> bool {
        self.generator.ended()
    }

    fn signal_release(&mut self) {
        self.generator.signal_release()
    }
}

impl<S, G> SIMDVoiceGenerator<S, SIMDSampleStereo<S>> for SIMDVoiceMonoToStereo<S, G>
where
    S: Simd,
    G: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
{
    fn next_sample(&mut self) -> SIMDSampleStereo<S> {
        let sample = self.generator.next_sample();
        SIMDSampleStereo(sample.0, sample.0)
    }
}
