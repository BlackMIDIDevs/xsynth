use std::marker::PhantomData;

use simdeez::prelude::*;

use crate::{
    effects::BiQuadFilter,
    voice::{ReleaseType, SIMDVoiceGenerator, VoiceControlData},
};

use super::{SIMDSampleMono, SIMDSampleStereo, VoiceGeneratorBase};

pub struct SIMDMonoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
{
    v: V,
    cutoff: BiQuadFilter,
    _s: PhantomData<S>,
}

impl<S, V> SIMDMonoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
{
    pub fn new(v: V, filter: &BiQuadFilter) -> Self {
        SIMDMonoVoiceCutoff {
            v,
            cutoff: filter.clone(),
            _s: PhantomData,
        }
    }
}

impl<S, V> VoiceGeneratorBase for SIMDMonoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
{
    #[inline(always)]
    fn ended(&self) -> bool {
        self.v.ended()
    }

    #[inline(always)]
    fn signal_release(&mut self, rel_type: ReleaseType) {
        self.v.signal_release(rel_type);
    }

    #[inline(always)]
    fn process_controls(&mut self, control: &VoiceControlData) {
        self.v.process_controls(control);
    }
}

impl<S, V> SIMDVoiceGenerator<S, SIMDSampleMono<S>> for SIMDMonoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
{
    #[inline(always)]
    fn next_sample(&mut self) -> SIMDSampleMono<S> {
        simd_invoke!(S, {
            let mut next_sample = self.v.next_sample();
            next_sample.0 = self.cutoff.process_simd::<S>(next_sample.0);
            next_sample
        })
    }
}

pub struct SIMDStereoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
{
    v: V,
    cutoff1: BiQuadFilter,
    cutoff2: BiQuadFilter,
    _s: PhantomData<S>,
}

impl<S, V> SIMDStereoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
{
    pub fn new(v: V, filter: &BiQuadFilter) -> Self {
        SIMDStereoVoiceCutoff {
            v,
            cutoff1: filter.clone(),
            cutoff2: filter.clone(),
            _s: PhantomData,
        }
    }
}

impl<S, V> VoiceGeneratorBase for SIMDStereoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
{
    #[inline(always)]
    fn ended(&self) -> bool {
        self.v.ended()
    }

    #[inline(always)]
    fn signal_release(&mut self, rel_type: ReleaseType) {
        self.v.signal_release(rel_type);
    }

    #[inline(always)]
    fn process_controls(&mut self, control: &VoiceControlData) {
        self.v.process_controls(control);
    }
}

impl<S, V> SIMDVoiceGenerator<S, SIMDSampleStereo<S>> for SIMDStereoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
{
    #[inline(always)]
    fn next_sample(&mut self) -> SIMDSampleStereo<S> {
        simd_invoke!(S, {
            let mut next_sample = self.v.next_sample();
            next_sample.0 = self.cutoff1.process_simd::<S>(next_sample.0);
            next_sample.1 = self.cutoff2.process_simd::<S>(next_sample.1);
            next_sample
        })
    }
}
