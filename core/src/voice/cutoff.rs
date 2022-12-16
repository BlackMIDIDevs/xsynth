use std::marker::PhantomData;

use simdeez::Simd;

use crate::{
    effects::CutoffFilterBase,
    voice::{SIMDVoiceGenerator, VoiceControlData},
};

use super::{SIMDSampleStereo, VoiceGeneratorBase};

pub struct SIMDStereoVoiceCutoff<F, S, V>
where
    F: Sync + Send + CutoffFilterBase,
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
{
    v: V,
    cutoff1: F,
    cutoff2: F,
    _s: PhantomData<S>,
}

impl<F, S, V> SIMDStereoVoiceCutoff<F, S, V>
where
    F: Sync + Send + CutoffFilterBase,
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
{
    pub fn new(v: V, filter: F) -> Self {
        SIMDStereoVoiceCutoff {
            v,
            cutoff1: filter.clone(),
            cutoff2: filter,
            _s: PhantomData,
        }
    }
}

impl<F, S, V> VoiceGeneratorBase for SIMDStereoVoiceCutoff<F, S, V>
where
    F: Sync + Send + CutoffFilterBase,
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
{
    #[inline(always)]
    fn ended(&self) -> bool {
        self.v.ended()
    }

    #[inline(always)]
    fn signal_release(&mut self) {
        self.v.signal_release();
    }

    #[inline(always)]
    fn process_controls(&mut self, control: &VoiceControlData) {
        self.v.process_controls(control);
    }
}

impl<F, S, V> SIMDVoiceGenerator<S, SIMDSampleStereo<S>> for SIMDStereoVoiceCutoff<F, S, V>
where
    F: Sync + Send + CutoffFilterBase,
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
{
    #[inline(always)]
    fn next_sample(&mut self) -> SIMDSampleStereo<S> {
        let mut next_sample = self.v.next_sample();
        next_sample.0 = self.cutoff1.process_sample_simd::<S>(next_sample.0);
        next_sample.1 = self.cutoff2.process_sample_simd::<S>(next_sample.1);
        next_sample
    }
}
