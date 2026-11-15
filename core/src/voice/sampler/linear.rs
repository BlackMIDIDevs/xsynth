use std::marker::PhantomData;

use simdeez::prelude::*;

use super::{gather, index_range, prefetch_range, SIMDSampleGrabber, SampleReader, GATHER_PAD};

pub struct SIMDLinearSampleGrabber<S: Simd, Reader: SampleReader> {
    sampler_reader: Reader,
    _s: PhantomData<S>,
}

impl<S: Simd, Reader: SampleReader> SIMDLinearSampleGrabber<S, Reader> {
    pub fn new(sampler_reader: Reader) -> Self {
        SIMDLinearSampleGrabber {
            sampler_reader,
            _s: PhantomData,
        }
    }
}

impl<S: Simd, Reader: SampleReader> SIMDSampleGrabber<S> for SIMDLinearSampleGrabber<S, Reader> {
    #[inline(always)]
    fn get(&mut self, indexes: S::Vi32, fractional: S::Vf32) -> S::Vf32 {
        simd_block!(S, {
            let ones = S::Vf32::set1(1.0f32);
            let blend = fractional;
            let mut values_first = ones;
            let mut values_second = ones;

            let (first, last) = index_range::<S>(indexes);
            match self.sampler_reader.contiguous(first, last + 1, 0) {
                Some(samples) => {
                    for i in 0..S::Vf32::WIDTH {
                        let index = indexes[i] as usize - first;
                        // Safety: every index is within first..=last and the
                        // slice covers first..=last + 1.
                        unsafe {
                            values_first[i] = *samples.get_unchecked(index);
                            values_second[i] = *samples.get_unchecked(index + 1);
                        }
                    }
                }
                None => {
                    for i in 0..S::Vf32::WIDTH {
                        let index = indexes[i] as usize;
                        values_first[i] = self.sampler_reader.get(index);
                        values_second[i] = self.sampler_reader.get(index + 1);
                    }
                }
            }

            let blended = values_first * (ones - blend) + values_second * blend;

            blended
        },)
    }

    #[inline(always)]
    fn get_block(
        &mut self,
        base: usize,
        offsets: S::Vi32,
        max_offset: usize,
        fractional: S::Vf32,
    ) -> S::Vf32 {
        simd_block!(S, {
            let ones = S::Vf32::set1(1.0f32);
            let (first, second) =
                match self
                    .sampler_reader
                    .contiguous(base, base + max_offset + 1, GATHER_PAD)
                {
                    // Safety: the slice covers every offset and offset + 1.
                    Some(samples) => unsafe {
                        (
                            gather::<S>(samples, offsets, max_offset),
                            gather::<S>(samples.get_unchecked(1..), offsets, max_offset),
                        )
                    },
                    None => {
                        let mut first = ones;
                        let mut second = ones;
                        for i in 0..S::Vf32::WIDTH {
                            let index = base + offsets[i] as u32 as usize;
                            first[i] = self.sampler_reader.get(index);
                            second[i] = self.sampler_reader.get(index + 1);
                        }
                        (first, second)
                    }
                };
            first * (ones - fractional) + second * fractional
        })
    }

    #[inline(always)]
    fn is_past_end(&self, pos: f64) -> bool {
        let pos = pos as usize;
        self.sampler_reader.is_past_end(pos)
    }

    fn signal_release(&mut self) {
        self.sampler_reader.signal_release();
    }

    #[inline(always)]
    fn prefetch(&self, pos: f64, len: usize) {
        if let Some((samples, index)) = self.sampler_reader.raw_position(pos as usize) {
            prefetch_range(samples, index, len);
        }
    }
}
