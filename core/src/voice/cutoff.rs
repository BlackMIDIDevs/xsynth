use std::marker::PhantomData;

use simdeez::Simd;

use crate::{
    effects::SingleChannelFilter,
    voice::{SIMDVoiceGenerator, VoiceControlData},
};

use soundfonts::FilterType;

use super::{SIMDSampleStereo, VoiceGeneratorBase};

pub struct SIMDStereoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
{
    v: V,
    cutoff1: SingleChannelFilter,
    cutoff2: SingleChannelFilter,
    _s: PhantomData<S>,
}

impl<S, V> SIMDStereoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
{
    pub fn new(v: V, filter_type: FilterType, sample_rate: f32, initial_cutoff: f32) -> Self {
        SIMDStereoVoiceCutoff {
            v,
            cutoff1: SingleChannelFilter::new(filter_type, initial_cutoff, sample_rate),
            cutoff2: SingleChannelFilter::new(filter_type, initial_cutoff, sample_rate),
            _s: PhantomData,
        }
    }
}

impl<S, V> VoiceGeneratorBase for SIMDStereoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
{
    fn ended(&self) -> bool {
        self.v.ended()
    }

    fn signal_release(&mut self) {
        self.v.signal_release();
    }

    fn process_controls(&mut self, control: &VoiceControlData) {
        self.v.process_controls(control);
    }
}

impl<S, V> SIMDVoiceGenerator<S, SIMDSampleStereo<S>> for SIMDStereoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
{
    fn next_sample(&mut self) -> SIMDSampleStereo<S> {
        let mut next_sample = self.v.next_sample();
        for i in 0..S::VF32_WIDTH {
            next_sample.0[i] = self.cutoff1.process_sample(next_sample.0[i]);
            next_sample.1[i] = self.cutoff2.process_sample(next_sample.1[i]);
        }
        next_sample
    }
}
