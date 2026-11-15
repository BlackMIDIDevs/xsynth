use std::marker::PhantomData;

use simdeez::prelude::*;
use simdeez::Simd;

use super::{gather, index_range, prefetch_range, SIMDSampleGrabber, SampleReader, GATHER_PAD};

pub struct SIMDNearestSampleGrabber<S: Simd, Reader: SampleReader> {
    sampler_reader: Reader,
    _s: PhantomData<S>,
}

impl<S: Simd, Reader: SampleReader> SIMDNearestSampleGrabber<S, Reader> {
    pub fn new(sampler_reader: Reader) -> Self {
        SIMDNearestSampleGrabber {
            sampler_reader,
            _s: PhantomData,
        }
    }
}

impl<S: Simd, Reader: SampleReader> SIMDSampleGrabber<S> for SIMDNearestSampleGrabber<S, Reader> {
    #[inline(always)]
    fn get(&mut self, indexes: S::Vi32, _: S::Vf32) -> S::Vf32 {
        simd_block!(S, unsafe {
            let mut values = S::Vf32::zeroes();

            let (first, last) = index_range::<S>(indexes);
            match self.sampler_reader.contiguous(first, last, 0) {
                Some(samples) => {
                    for i in 0..S::Vf32::WIDTH {
                        let index = indexes.get_unchecked(i) as usize - first;
                        // Safety: every index is within first..=last, which the slice covers.
                        *values.get_unchecked_mut(i) = *samples.get_unchecked(index);
                    }
                }
                None => {
                    for i in 0..S::Vf32::WIDTH {
                        let index = indexes.get_unchecked(i) as usize;
                        *values.get_unchecked_mut(i) = self.sampler_reader.get(index);
                    }
                }
            }

            values
        })
    }

    #[inline(always)]
    fn get_block(
        &mut self,
        base: usize,
        offsets: S::Vi32,
        max_offset: usize,
        _: S::Vf32,
    ) -> S::Vf32 {
        match self
            .sampler_reader
            .contiguous(base, base + max_offset, GATHER_PAD)
        {
            // Safety: the slice covers every offset and the gather padding.
            Some(samples) => unsafe { gather::<S>(samples, offsets, max_offset) },
            None => simd_block!(S, {
                let mut values = S::Vf32::zeroes();
                for i in 0..S::Vf32::WIDTH {
                    values[i] = self.sampler_reader.get(base + offsets[i] as u32 as usize);
                }
                values
            }),
        }
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
