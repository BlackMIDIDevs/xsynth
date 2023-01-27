use std::marker::PhantomData;

use simdeez::prelude::*;
use simdeez::Simd;

use super::{BufferSampler, SIMDSampleGrabber, SampleReader};

pub struct SIMDNearestSampleGrabber<S: Simd, Sampler: BufferSampler> {
    sampler_reader: SampleReader<Sampler>,
    _s: PhantomData<S>,
}

impl<S: Simd, Sampler: BufferSampler> SIMDNearestSampleGrabber<S, Sampler> {
    pub fn new(sampler_reader: SampleReader<Sampler>) -> Self {
        SIMDNearestSampleGrabber {
            sampler_reader,
            _s: PhantomData,
        }
    }
}

impl<S: Simd, Sampler: BufferSampler> SIMDSampleGrabber<S>
    for SIMDNearestSampleGrabber<S, Sampler>
{
    fn get(&self, indexes: S::Vi32, _: S::Vf32) -> S::Vf32 {
        simd_invoke!(S, unsafe {
            let mut values = S::Vf32::zeroes();

            for i in 0..S::Vf32::WIDTH {
                let index = indexes.get_unchecked(i) as usize;
                *values.get_unchecked_mut(i) = self.sampler_reader.get(index);
            }

            values
        })
    }

    fn is_past_end(&self, pos: f64) -> bool {
        let pos = pos as usize;
        self.sampler_reader.is_past_end(pos)
    }
}
