use std::marker::PhantomData;

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
        let ones = unsafe { S::set1_ps(1.0) };
        let mut values = ones;

        for i in 0..S::VF32_WIDTH {
            let index = indexes[i] as usize;
            values[i] = self.sampler_reader.get(index);
        }

        values
    }

    fn is_past_end(&self, pos: f64) -> bool {
        let pos = pos as usize;
        self.sampler_reader.is_past_end(pos)
    }
}

pub struct SIMDLinearSampleGrabber<S: Simd, Sampler: BufferSampler> {
    sampler_reader: SampleReader<Sampler>,
    _s: PhantomData<S>,
}

impl<S: Simd, Sampler: BufferSampler> SIMDLinearSampleGrabber<S, Sampler> {
    pub fn new(sampler_reader: SampleReader<Sampler>) -> Self {
        SIMDLinearSampleGrabber {
            sampler_reader,
            _s: PhantomData,
        }
    }
}

impl<S: Simd, Sampler: BufferSampler> SIMDSampleGrabber<S> for SIMDLinearSampleGrabber<S, Sampler> {
    fn get(&self, indexes: S::Vi32, fractional: S::Vf32) -> S::Vf32 {
        let ones = unsafe { S::set1_ps(1.0) };
        let blend = fractional;
        let mut values_first = ones;
        let mut values_second = ones;

        for i in 0..S::VF32_WIDTH {
            let index = indexes[i] as usize;
            values_first[i] = self.sampler_reader.get(index);
            values_second[i] = self.sampler_reader.get(index + 1);
        }

        let blended = values_first * blend + values_second * (ones - blend);

        blended
    }

    fn is_past_end(&self, pos: f64) -> bool {
        let pos = pos as usize;
        self.sampler_reader.is_past_end(pos)
    }
}
