use std::marker::PhantomData;

use simdeez::prelude::*;
use simdeez::Simd;

use super::{BufferSampler, SIMDSampleGrabber, SampleReader};

pub struct SIMDNearestSampleGrabber<S: Simd, Sampler: BufferSampler, Reader: SampleReader<Sampler>>
{
    sampler_reader: Reader,
    _s: PhantomData<S>,
    _sampler: PhantomData<Sampler>,
}

impl<S: Simd, Sampler: BufferSampler, Reader: SampleReader<Sampler>>
    SIMDNearestSampleGrabber<S, Sampler, Reader>
{
    pub fn new(sampler_reader: Reader) -> Self {
        SIMDNearestSampleGrabber {
            sampler_reader,
            _s: PhantomData,
            _sampler: PhantomData,
        }
    }
}

impl<S: Simd, Sampler: BufferSampler, Reader: SampleReader<Sampler>> SIMDSampleGrabber<S>
    for SIMDNearestSampleGrabber<S, Sampler, Reader>
{
    fn get(&mut self, indexes: S::Vi32, _: S::Vf32) -> S::Vf32 {
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

    fn signal_release(&mut self) {
        self.sampler_reader.signal_release();
    }
}
