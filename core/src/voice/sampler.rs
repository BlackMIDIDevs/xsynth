use std::{marker::PhantomData, sync::Arc};

use simdeez::prelude::*;

use crate::soundfont::LoopParams;
use crate::voice::{ReleaseType, VoiceControlData};

use super::{
    lane_numbers, SIMDSampleMono, SIMDSampleStereo, SIMDVoiceGenerator, VoiceGeneratorBase,
};

mod linear;
pub use linear::*;

mod nearest;
pub use nearest::*;

// I believe some terminology reference is relevant for this one.
//
// BufferSampler: Something that grabs a sample based on an index
//
// SampleReader: Something that grabs the sample value at an arbitrary index,
// and implements sample start/end/looping
//
// SIMDSampleGrabber: Something that takes a SIMD array of float64 locations and
// returns a SIMD array of f32 interpolated sample values

// Base traits

pub trait BufferSampler: Send + Sync {
    fn get(&self, pos: usize) -> f32;
    fn length(&self) -> usize;
    /// The raw samples, where `get(i) == as_slice()[i]` for every in-bounds `i`.
    fn as_slice(&self) -> &[f32];
}

pub trait SIMDSampleGrabber<S: Simd>: Send + Sync {
    /// Indexes: the rounded index of the sample
    ///
    /// Fractional: The fractional part of the index, i.e. the 0-1 range decimal
    fn get(&mut self, indexes: S::Vi32, fractional: S::Vf32) -> S::Vf32;

    /// Samples the positions `base + offsets`, with `max_offset` the largest
    /// offset. Equivalent to `get` with absolute indexes, but can read the
    /// sample buffer directly when the positions are stored contiguously.
    fn get_block(
        &mut self,
        base: usize,
        offsets: S::Vi32,
        max_offset: usize,
        fractional: S::Vf32,
    ) -> S::Vf32;

    fn is_past_end(&self, pos: f64) -> bool;

    fn signal_release(&mut self);

    /// Prefetches `len` samples from a playback position.
    #[inline(always)]
    fn prefetch(&self, _pos: f64, _len: usize) {}
}

/// Samples that `gather` may read past the largest offset.
pub(crate) const GATHER_PAD: usize = 15;

/// Reads `samples[offsets[i]]` into every lane, with `max_offset` the largest
/// offset.
///
/// # Safety
/// Every offset must be a valid index into `samples`, and `samples` must hold
/// at least `max_offset + 1 + GATHER_PAD` values when `max_offset < 16`.
#[inline(always)]
pub(crate) unsafe fn gather<S: Simd>(
    samples: &[f32],
    offsets: S::Vi32,
    max_offset: usize,
) -> S::Vf32 {
    // With AVX2 enabled for the whole build, offsets that span at most 16
    // samples (a pitch of up to 2x) are read with two vector loads and lane
    // permutes instead of eight scalar loads.
    #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
    if S::Vf32::WIDTH == 8
        && std::mem::size_of::<S::Vi32>() == 32
        && std::mem::size_of::<S::Vf32>() == 32
        && max_offset < 16
    {
        use core::arch::x86_64::*;
        let offsets: __m256i = std::mem::transmute_copy(&offsets);
        let ptr = samples.as_ptr();
        let low = _mm256_permutevar8x32_ps(_mm256_loadu_ps(ptr), offsets);
        let high = _mm256_permutevar8x32_ps(_mm256_loadu_ps(ptr.add(8)), offsets);
        let use_high = _mm256_castsi256_ps(_mm256_cmpgt_epi32(offsets, _mm256_set1_epi32(7)));
        let values = _mm256_blendv_ps(low, high, use_high);
        return std::mem::transmute_copy(&values);
    }

    #[cfg(not(all(target_arch = "x86_64", target_feature = "avx2")))]
    let _ = max_offset;

    let mut values = S::Vf32::zeroes();
    for i in 0..S::Vf32::WIDTH {
        *values.get_unchecked_mut(i) = *samples.get_unchecked(offsets[i] as u32 as usize);
    }
    values
}

/// The playback positions of one vector of samples, relative to `base`.
pub(crate) struct VectorPositions<S: Simd> {
    pub base: usize,
    pub offsets: S::Vi32,
    pub max_offset: usize,
    pub fractional: S::Vf32,
}

/// Advances `time` by one vector of the given per-sample speeds and returns
/// the positions of that vector.
///
/// Uniform non-negative speeds (the pitch is constant within a vector) take a
/// vectorized path where each lane is `time + lane * speed`; otherwise the
/// speeds are accumulated lane by lane.
#[inline(always)]
pub(crate) fn vector_positions<S: Simd>(
    time: &mut f64,
    speed: S::Vf32,
    known_uniform: bool,
) -> VectorPositions<S> {
    simd_block!(S, {
        let width = S::Vf32::WIDTH;
        let mut first = speed[0];
        let uniform = if known_uniform {
            first = first.max(0.0);
            true
        } else {
            let mut uniform = first >= 0.0;
            for i in 1..width {
                uniform &= speed[i] == first;
            }
            uniform
        };
        // Safety: the playback position is finite, non-negative and far below
        // usize::MAX, so the conversion cannot overflow.
        let base: usize = unsafe { time.to_int_unchecked() };
        if uniform {
            let start = (*time - base as f64) as f32;
            let position = S::Vf32::set1(first).mul_add(lane_numbers::<S>(), S::Vf32::set1(start));
            let floor = position.floor();
            let offsets = floor.cast_i32();
            *time += first as f64 * width as f64;
            // The last lane holds the largest offset. With FMA enabled for the
            // whole build, the vector `mul_add` above is fused, so the same
            // scalar operation gives the identical value without waiting for
            // the vector conversion and lane extract.
            #[cfg(target_feature = "fma")]
            let max_offset = first.mul_add((width - 1) as f32, start).floor() as u32 as usize;
            #[cfg(not(target_feature = "fma"))]
            let max_offset = offsets[width - 1] as u32 as usize;
            debug_assert_eq!(max_offset, offsets[width - 1] as u32 as usize);
            VectorPositions {
                base,
                max_offset,
                offsets,
                fractional: position - floor,
            }
        } else {
            let mut offsets = S::Vi32::zeroes();
            let mut fractional = S::Vf32::zeroes();
            let mut offset = 0.0f64;
            let mut max_offset = 0;
            for i in 0..width {
                // Positions never go below the start of the vector, as the
                // per-sample speed is clamped to be non-negative.
                let time = *time + offset;
                let index: usize = unsafe { time.to_int_unchecked() };
                offsets[i] = (index - base) as i32;
                fractional[i] = (time - index as f64) as f32;
                max_offset = max_offset.max(index - base);
                offset += speed[i].max(0.0) as f64;
            }
            *time += offset;
            VectorPositions {
                base,
                offsets,
                max_offset,
                fractional,
            }
        }
    })
}

// F32 sampler

pub struct F32BufferSampler(Arc<[f32]>);

impl BufferSampler for F32BufferSampler {
    #[inline(always)]
    fn get(&self, pos: usize) -> f32 {
        match self.0.get(pos) {
            Some(v) => *v,
            None => 0.0,
        }
    }

    fn length(&self) -> usize {
        self.0.len()
    }

    #[inline(always)]
    fn as_slice(&self) -> &[f32] {
        &self.0
    }
}

// Generalized enum sampler

pub enum BufferSamplers {
    F32(F32BufferSampler),
}

impl BufferSamplers {
    #[inline(always)]
    pub fn new_f32(sample: Arc<[f32]>) -> BufferSamplers {
        BufferSamplers::F32(F32BufferSampler(sample))
    }
}

impl BufferSampler for BufferSamplers {
    #[inline(always)]
    fn get(&self, pos: usize) -> f32 {
        match self {
            BufferSamplers::F32(sampler) => sampler.get(pos),
        }
    }

    fn length(&self) -> usize {
        match self {
            BufferSamplers::F32(sampler) => sampler.length(),
        }
    }

    #[inline(always)]
    fn as_slice(&self) -> &[f32] {
        match self {
            BufferSamplers::F32(sampler) => sampler.as_slice(),
        }
    }
}

// Enum sampler reader

pub trait SampleReader: Send + Sync {
    fn get(&mut self, pos: usize) -> f32;
    fn is_past_end(&self, pos: usize) -> bool;
    fn signal_release(&mut self);

    /// Fast path for reading every position in `first..=last`. Returns a slice
    /// where `slice[p - first]` equals `get(p)`, leaving the reader in the
    /// same state as calling `get` on each position in order, or `None` if the
    /// range is not stored contiguously (loop wrap or end of the sample).
    ///
    /// The slice also covers `pad` more positions, which must be readable but
    /// do not count as read.
    fn contiguous(&mut self, first: usize, last: usize, pad: usize) -> Option<&[f32]>;

    /// The raw samples and the index in them of a playback position, if the
    /// position maps directly to the buffer.
    fn raw_position(&self, pos: usize) -> Option<(&[f32], usize)>;
}

/// The number of samples a render of `frames` samples reads, capped so that
/// long buffers do not prefetch more than the caches hold.
#[inline(always)]
fn prefetch_len(frames: usize, pitch: &impl VoiceGeneratorBase) -> usize {
    ((frames as f32 * pitch.speed_hint()) as usize + 2).min(256)
}

/// Prefetches `len` samples starting at `samples[index]`.
#[inline(always)]
pub(crate) fn prefetch_range(samples: &[f32], index: usize, len: usize) {
    let mut offset = 0;
    while offset < len {
        prefetch_index(samples, index + offset);
        offset += 16;
    }
    prefetch_index(samples, index + len);
}

/// Prefetches the cache line holding `samples[index]`.
#[inline(always)]
pub(crate) fn prefetch_index(samples: &[f32], index: usize) {
    #[cfg(target_arch = "x86_64")]
    // Safety: prefetching never faults, even past the end of the slice.
    unsafe {
        use core::arch::x86_64::{_mm_prefetch, _MM_HINT_T0};
        _mm_prefetch::<_MM_HINT_T0>(samples.as_ptr().wrapping_add(index) as *const i8);
    }
    #[cfg(not(target_arch = "x86_64"))]
    let _ = (samples, index);
}

/// Returns `buffer[start..=start + len]` if it is in bounds.
#[inline(always)]
fn sub_slice(buffer: &[f32], start: usize, len: usize) -> Option<&[f32]> {
    buffer.get(start..start.checked_add(len)?.checked_add(1)?)
}

pub struct SampleReaderNoLoop<Sampler: BufferSampler> {
    buffer: Sampler,
    // u32 keeps the voice state small; samples are far shorter than 2^32.
    length: u32,
    offset: u32,
}

impl<Sampler: BufferSampler> SampleReaderNoLoop<Sampler> {
    pub fn new(buffer: Sampler, loop_params: LoopParams) -> Self {
        let stop = loop_params
            .stop
            .map(|stop| stop as usize)
            .unwrap_or_else(|| buffer.length());
        let length = stop as u32;
        Self {
            buffer,
            length,
            offset: loop_params.offset,
        }
    }
}

impl<Sampler: BufferSampler> SampleReader for SampleReaderNoLoop<Sampler> {
    fn get(&mut self, pos: usize) -> f32 {
        self.buffer.get(pos + (self.offset as usize))
    }

    fn is_past_end(&self, pos: usize) -> bool {
        pos >= (self.length as usize).saturating_sub(self.offset as usize)
    }

    fn signal_release(&mut self) {}

    #[inline(always)]
    fn contiguous(&mut self, first: usize, last: usize, pad: usize) -> Option<&[f32]> {
        sub_slice(
            self.buffer.as_slice(),
            first + (self.offset as usize),
            last + pad - first,
        )
    }

    #[inline(always)]
    fn raw_position(&self, pos: usize) -> Option<(&[f32], usize)> {
        Some((self.buffer.as_slice(), pos + (self.offset as usize)))
    }
}

pub struct SampleReaderLoop<Sampler: BufferSampler> {
    buffer: Sampler,
    offset: u32,
    loop_start: u32,
    loop_end: u32,
}

impl<Sampler: BufferSampler> SampleReaderLoop<Sampler> {
    pub fn new(buffer: Sampler, loop_params: LoopParams) -> Self {
        Self {
            buffer,
            offset: loop_params.offset,
            loop_start: loop_params.start,
            loop_end: loop_params.end,
        }
    }
}

impl<Sampler: BufferSampler> SampleReader for SampleReaderLoop<Sampler> {
    fn get(&mut self, pos: usize) -> f32 {
        let mut pos = pos + (self.offset as usize);
        let end = self.loop_end as usize;
        let start = self.loop_start as usize;

        if pos > end {
            pos = (pos - end - 1) % (end - start) + start;
        }

        self.buffer.get(pos)
    }

    fn is_past_end(&self, _pos: usize) -> bool {
        false
    }

    fn signal_release(&mut self) {}

    #[inline(always)]
    fn contiguous(&mut self, first: usize, last: usize, pad: usize) -> Option<&[f32]> {
        if last + pad + (self.offset as usize) > (self.loop_end as usize) {
            return None;
        }
        sub_slice(
            self.buffer.as_slice(),
            first + (self.offset as usize),
            last + pad - first,
        )
    }

    #[inline(always)]
    fn raw_position(&self, pos: usize) -> Option<(&[f32], usize)> {
        let pos = pos + (self.offset as usize);
        (pos <= (self.loop_end as usize)).then(|| (self.buffer.as_slice(), pos))
    }
}

pub struct SampleReaderLoopSustain<Sampler: BufferSampler> {
    buffer: Sampler,
    length: usize,
    offset: usize,
    loop_start: usize,
    loop_end: usize,
    last: usize,
    is_released: bool,
}

impl<Sampler: BufferSampler> SampleReaderLoopSustain<Sampler> {
    pub fn new(buffer: Sampler, loop_params: LoopParams) -> Self {
        let stop = loop_params
            .stop
            .map(|stop| stop as usize)
            .unwrap_or_else(|| buffer.length());
        let length = stop;
        Self {
            buffer,
            length,
            offset: loop_params.offset as usize,
            loop_start: loop_params.start as usize,
            loop_end: loop_params.end as usize,
            last: 0,
            is_released: false,
        }
    }
}

impl<Sampler: BufferSampler> SampleReader for SampleReaderLoopSustain<Sampler> {
    fn get(&mut self, pos: usize) -> f32 {
        let mut pos = pos + self.offset;
        let end = self.loop_end;
        let start = self.loop_start;

        if !self.is_released {
            self.last = pos;
            if pos > end {
                pos = (pos - end - 1) % (end - start) + start;
            }
        } else {
            pos = pos - self.last + self.loop_end;
        }

        self.buffer.get(pos)
    }

    fn is_past_end(&self, pos: usize) -> bool {
        pos >= self.length.saturating_sub(self.last + self.offset)
    }

    fn signal_release(&mut self) {
        self.is_released = true;
    }

    #[inline(always)]
    fn contiguous(&mut self, first: usize, last: usize, pad: usize) -> Option<&[f32]> {
        let len = last + pad - first;
        if !self.is_released {
            if last + pad + self.offset > self.loop_end {
                return None;
            }
            self.last = last + self.offset;
            sub_slice(self.buffer.as_slice(), first + self.offset, len)
        } else {
            let start = (first + self.offset).checked_sub(self.last)? + self.loop_end;
            sub_slice(self.buffer.as_slice(), start, len)
        }
    }

    #[inline(always)]
    fn raw_position(&self, pos: usize) -> Option<(&[f32], usize)> {
        let pos = pos + self.offset;
        if !self.is_released {
            (pos <= self.loop_end).then(|| (self.buffer.as_slice(), pos))
        } else {
            Some((
                self.buffer.as_slice(),
                pos.checked_sub(self.last)? + self.loop_end,
            ))
        }
    }
}

/// The lowest and highest sample index of a vector of indexes.
#[inline(always)]
pub(crate) fn index_range<S: Simd>(indexes: S::Vi32) -> (usize, usize) {
    let mut first = indexes[0];
    let mut last = first;
    for i in 1..S::Vi32::WIDTH {
        let index = indexes[i];
        first = first.min(index);
        last = last.max(index);
    }
    (first as u32 as usize, last as u32 as usize)
}

// Sample grabbers enum

pub enum SIMDSampleGrabbers<S: Simd, Reader: SampleReader> {
    Nearest(SIMDNearestSampleGrabber<S, Reader>),
    Linear(SIMDLinearSampleGrabber<S, Reader>),
}

impl<S: Simd, Reader: SampleReader> SIMDSampleGrabbers<S, Reader> {
    pub fn nearest(reader: Reader) -> Self {
        SIMDSampleGrabbers::Nearest(SIMDNearestSampleGrabber::new(reader))
    }

    pub fn linear(reader: Reader) -> Self {
        SIMDSampleGrabbers::Linear(SIMDLinearSampleGrabber::new(reader))
    }
}

impl<S: Simd, Reader: SampleReader> SIMDSampleGrabber<S> for SIMDSampleGrabbers<S, Reader> {
    #[inline(always)]
    fn get(&mut self, indexes: S::Vi32, fractional: S::Vf32) -> S::Vf32 {
        match self {
            SIMDSampleGrabbers::Linear(grabber) => grabber.get(indexes, fractional),
            SIMDSampleGrabbers::Nearest(grabber) => grabber.get(indexes, fractional),
        }
    }

    #[inline(always)]
    fn get_block(
        &mut self,
        base: usize,
        offsets: S::Vi32,
        max_offset: usize,
        fractional: S::Vf32,
    ) -> S::Vf32 {
        match self {
            SIMDSampleGrabbers::Linear(grabber) => {
                grabber.get_block(base, offsets, max_offset, fractional)
            }
            SIMDSampleGrabbers::Nearest(grabber) => {
                grabber.get_block(base, offsets, max_offset, fractional)
            }
        }
    }

    #[inline(always)]
    fn is_past_end(&self, pos: f64) -> bool {
        match self {
            SIMDSampleGrabbers::Linear(grabber) => grabber.is_past_end(pos),
            SIMDSampleGrabbers::Nearest(grabber) => grabber.is_past_end(pos),
        }
    }

    #[inline(always)]
    fn signal_release(&mut self) {
        match self {
            SIMDSampleGrabbers::Linear(grabber) => grabber.signal_release(),
            SIMDSampleGrabbers::Nearest(grabber) => grabber.signal_release(),
        }
    }

    #[inline(always)]
    fn prefetch(&self, pos: f64, len: usize) {
        match self {
            SIMDSampleGrabbers::Linear(grabber) => grabber.prefetch(pos, len),
            SIMDSampleGrabbers::Nearest(grabber) => grabber.prefetch(pos, len),
        }
    }
}

// Sampler generator

pub struct SIMDMonoVoiceSampler<S, Pitch, Grabber>
where
    S: Simd,
    Pitch: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
    Grabber: SIMDSampleGrabber<S>,
{
    grabber: Grabber,

    pitch_gen: Pitch,

    time: f64,

    _s: PhantomData<S>,
}

impl<S, Pitch, Grabber> SIMDMonoVoiceSampler<S, Pitch, Grabber>
where
    S: Simd,
    Pitch: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
    Grabber: SIMDSampleGrabber<S>,
{
    pub fn new(grabber: Grabber, pitch_gen: Pitch) -> Self {
        SIMDMonoVoiceSampler {
            grabber,
            pitch_gen,
            time: 0.0,
            _s: PhantomData,
        }
    }
}

impl<S, Pitch, Grabber> VoiceGeneratorBase for SIMDMonoVoiceSampler<S, Pitch, Grabber>
where
    S: Simd,
    Pitch: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
    Grabber: SIMDSampleGrabber<S>,
{
    #[inline(always)]
    fn ended(&self) -> bool {
        self.grabber.is_past_end(self.time)
    }

    #[inline(always)]
    fn signal_release(&mut self, rel_type: ReleaseType) {
        self.pitch_gen.signal_release(rel_type);
        self.grabber.signal_release();
    }

    #[inline(always)]
    fn process_controls(&mut self, control: &VoiceControlData) {
        self.pitch_gen.process_controls(control);
    }

    #[inline(always)]
    fn prefetch(&self, frames: usize) {
        self.grabber
            .prefetch(self.time, prefetch_len(frames, &self.pitch_gen))
    }
}

impl<S, Pitch, Grabber> SIMDVoiceGenerator<S, SIMDSampleMono<S>>
    for SIMDMonoVoiceSampler<S, Pitch, Grabber>
where
    S: Simd,
    Pitch: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
    Grabber: SIMDSampleGrabber<S>,
{
    #[inline(always)]
    fn next_sample(&mut self) -> SIMDSampleMono<S> {
        let speed = self.pitch_gen.next_sample().0;
        let p = vector_positions::<S>(&mut self.time, speed, Pitch::UNIFORM);
        SIMDSampleMono(
            self.grabber
                .get_block(p.base, p.offsets, p.max_offset, p.fractional),
        )
    }
}

pub struct SIMDStereoVoiceSampler<S, Pitch, Grabber>
where
    S: Simd,
    Pitch: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
    Grabber: SIMDSampleGrabber<S>,
{
    grabber_left: Grabber,
    grabber_right: Grabber,

    pitch_gen: Pitch,

    time: f64,

    _s: PhantomData<S>,
}

impl<S, Pitch, Grabber> SIMDStereoVoiceSampler<S, Pitch, Grabber>
where
    S: Simd,
    Pitch: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
    Grabber: SIMDSampleGrabber<S>,
{
    pub fn new(grabber_left: Grabber, grabber_right: Grabber, pitch_gen: Pitch) -> Self {
        SIMDStereoVoiceSampler {
            grabber_left,
            grabber_right,
            pitch_gen,
            time: 0.0,
            _s: PhantomData,
        }
    }
}

impl<S, Pitch, Grabber> VoiceGeneratorBase for SIMDStereoVoiceSampler<S, Pitch, Grabber>
where
    S: Simd,
    Pitch: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
    Grabber: SIMDSampleGrabber<S>,
{
    #[inline(always)]
    fn ended(&self) -> bool {
        self.grabber_left.is_past_end(self.time) || self.grabber_right.is_past_end(self.time)
    }

    #[inline(always)]
    fn signal_release(&mut self, rel_type: ReleaseType) {
        self.pitch_gen.signal_release(rel_type);
        self.grabber_left.signal_release();
        self.grabber_right.signal_release();
    }

    #[inline(always)]
    fn process_controls(&mut self, control: &VoiceControlData) {
        self.pitch_gen.process_controls(control);
    }

    #[inline(always)]
    fn prefetch(&self, frames: usize) {
        let len = prefetch_len(frames, &self.pitch_gen);
        self.grabber_left.prefetch(self.time, len);
        self.grabber_right.prefetch(self.time, len)
    }
}

impl<S, Pitch, Grabber> SIMDVoiceGenerator<S, SIMDSampleStereo<S>>
    for SIMDStereoVoiceSampler<S, Pitch, Grabber>
where
    S: Simd,
    Pitch: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
    Grabber: SIMDSampleGrabber<S>,
{
    #[inline(always)]
    fn next_sample(&mut self) -> SIMDSampleStereo<S> {
        let speed = self.pitch_gen.next_sample().0;
        let p = vector_positions::<S>(&mut self.time, speed, Pitch::UNIFORM);
        let left = self
            .grabber_left
            .get_block(p.base, p.offsets, p.max_offset, p.fractional);
        let right = self
            .grabber_right
            .get_block(p.base, p.offsets, p.max_offset, p.fractional);
        SIMDSampleStereo(left, right)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use simdeez::simd_runtime_generate;

    #[test]
    fn gather_matches_indexing() {
        simd_runtime_generate!(
            fn run() {
                let samples: Vec<f32> = (0..64).map(|i| i as f32 * 0.5 - 3.0).collect();
                for speed in [0.0f32, 0.3, 0.999, 1.0, 1.37, 1.99, 2.0, 2.5, 4.0] {
                    for start in [0.0f32, 0.25, 0.75] {
                        let mut offsets = S::Vi32::zeroes();
                        let mut max_offset = 0;
                        for i in 0..S::Vi32::WIDTH {
                            let offset = (start + speed * i as f32) as i32;
                            offsets[i] = offset;
                            max_offset = max_offset.max(offset as usize);
                        }
                        // Safety: the slice holds every offset plus GATHER_PAD.
                        let values = unsafe {
                            gather::<S>(
                                &samples[..max_offset + 1 + GATHER_PAD],
                                offsets,
                                max_offset,
                            )
                        };
                        for i in 0..S::Vf32::WIDTH {
                            assert_eq!(values[i], samples[offsets[i] as usize], "speed {speed}");
                        }
                    }
                }
            }
        );
        run();
    }
}
