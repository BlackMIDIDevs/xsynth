use std::marker::PhantomData;

use simdeez::prelude::*;

use crate::voice::lane_numbers;

// The lerp equation is `start + (end - start) * factor`
// We store: start, length (= end - start)
pub(super) struct SIMDLerper<T: Simd> {
    start: f32,
    length: f32,
    _t: PhantomData<T>,
}

impl<T: Simd> SIMDLerper<T> {
    pub fn new(start: f32, end: f32) -> Self {
        SIMDLerper {
            start,
            length: end - start,
            _t: PhantomData,
        }
    }

    pub fn lerp(&self, factor: f32) -> f32 {
        self.start + self.length * factor
    }

    pub fn lerp_simd(&self, factor: T::Vf32) -> T::Vf32 {
        simd_block!(
            T,
            T::Vf32::set1(self.start) + T::Vf32::set1(self.length) * factor
        )
    }
}

pub(super) struct SIMDLerperConcave<T: Simd> {
    length: f32,
    end: f32,
    _t: PhantomData<T>,
}

impl<T: Simd> SIMDLerperConcave<T> {
    pub fn new(start: f32, end: f32) -> Self {
        SIMDLerperConcave {
            length: start - end,
            end,
            _t: PhantomData,
        }
    }

    pub fn lerp(&self, factor: f32) -> f32 {
        let mult = (1.0 - factor).powi(8);
        self.length * mult + self.end
    }

    pub fn lerp_simd(&self, factor: T::Vf32) -> T::Vf32 {
        simd_block!(T, {
            let one = T::Vf32::set1(1.0);
            let r1 = one - factor;
            let r2 = r1 * r1;
            let r3 = r2 * r2;
            let mult = r3 * r3;
            T::Vf32::set1(self.length) * mult + T::Vf32::set1(self.end)
        })
    }
}

pub(super) struct SIMDLerperConvex<T: Simd> {
    start: f32,
    length: f32,
    _t: PhantomData<T>,
}

impl<T: Simd> SIMDLerperConvex<T> {
    pub fn new(start: f32, end: f32) -> Self {
        SIMDLerperConvex {
            start,
            length: end - start,
            _t: PhantomData,
        }
    }

    pub fn lerp(&self, factor: f32) -> f32 {
        let mult = factor.powi(8);
        self.length * mult + self.start
    }

    pub fn lerp_simd(&self, factor: T::Vf32) -> T::Vf32 {
        simd_block!(T, {
            let r1 = factor * factor;
            let r2 = r1 * r1;
            let mult = r2 * r2;
            T::Vf32::set1(self.length) * mult + T::Vf32::set1(self.start)
        })
    }
}

pub(super) struct StageTime<T: Simd> {
    /// Time of the first sample in the current SIMD array
    stage_time: f32,
    stage_end_time_f32: f32,
    _t: PhantomData<T>,
}

impl<T: Simd> StageTime<T> {
    pub fn new(start_offset: u32, stage_end_time: u32) -> Self {
        StageTime {
            stage_time: start_offset as f32,
            stage_end_time_f32: stage_end_time as f32,
            _t: PhantomData,
        }
    }

    #[inline(always)]
    pub fn increment(&mut self) {
        self.stage_time += T::Vf32::WIDTH as f32;
    }

    #[inline(always)]
    pub fn increment_by(&mut self, by: u32) {
        self.stage_time += by as f32;
    }

    #[inline(always)]
    /// Is the upper most value in the SIMD array past the end?
    pub fn is_ending(&self) -> bool {
        self.simd_array_end_f32() >= self.stage_end_time_f32
    }

    #[inline(always)]
    /// Is the SIMD array intersecting the end? Or has it completely passed the end
    pub fn is_intersecting_end(&self) -> bool {
        self.is_ending() && self.simd_array_start_f32() < self.stage_end_time_f32
    }

    #[inline(always)]
    pub fn raw_simd_array(&self) -> T::Vf32 {
        simd_block!(T, T::Vf32::set1(self.stage_time) + lane_numbers::<T>())
    }

    #[inline(always)]
    pub fn progress_simd_array(&self) -> T::Vf32 {
        simd_block!(
            T,
            self.raw_simd_array() / T::Vf32::set1(self.stage_end_time_f32)
        )
    }

    #[inline(always)]
    pub fn simd_array_start_f32(&self) -> f32 {
        self.stage_time
    }

    #[inline(always)]
    pub fn stage_end_time_f32(&self) -> f32 {
        self.stage_end_time_f32
    }

    #[inline(always)]
    pub fn simd_array_end_f32(&self) -> f32 {
        self.stage_time + (T::Vf32::WIDTH - 1) as f32
    }

    #[allow(unused)]
    #[inline(always)]
    pub fn simd_array_start(&self) -> u32 {
        self.simd_array_start_f32() as u32
    }

    #[allow(unused)]
    #[inline(always)]
    pub fn simd_array_end(&self) -> u32 {
        self.simd_array_end_f32() as u32
    }
}
