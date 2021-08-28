use std::{marker::PhantomData, sync::Arc};

mod linear;
pub use linear::*;
use simdeez::Simd;

use super::{SIMDSampleMono, SIMDSampleStereo, SIMDVoiceGenerator, VoiceGeneratorBase};

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
}

pub trait SIMDSampleGrabber<S: Simd>: Send + Sync {
    /// Indexes: the rounded index of the sample
    ///
    /// Fractional: The fractional part of the index, i.e. the 0-1 range decimal
    fn get(&self, indexes: S::Vi32, fractional: S::Vf32) -> S::Vf32;

    fn is_past_end(&self, pos: f64) -> bool;
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
}

// Enum sampler reader

pub struct SampleReader<Sampler: BufferSampler> {
    buffer: Sampler,
    length: Option<usize>,
    // TODO: add start/end/loop points
}

impl<Sampler: BufferSampler> SampleReader<Sampler> {
    pub fn new(buffer: Sampler) -> Self {
        let length = Some(buffer.length());
        SampleReader { buffer, length }
    }

    pub fn get(&self, pos: usize) -> f32 {
        self.buffer.get(pos)
    }

    fn is_past_end(&self, pos: usize) -> bool {
        if let Some(len) = self.length {
            pos >= len
        } else {
            false
        }
    }
}

// Sample grabbers enum

pub enum SIMDSampleGrabbers<S: Simd, Sampler: BufferSampler> {
    Nearest(SIMDNearestSampleGrabber<S, Sampler>),
    Linear(SIMDLinearSampleGrabber<S, Sampler>),
}

impl<S: Simd, Sampler: BufferSampler> SIMDSampleGrabbers<S, Sampler> {
    pub fn nearest(reader: SampleReader<Sampler>) -> Self {
        SIMDSampleGrabbers::Nearest(SIMDNearestSampleGrabber::new(reader))
    }

    pub fn linear(reader: SampleReader<Sampler>) -> Self {
        SIMDSampleGrabbers::Linear(SIMDLinearSampleGrabber::new(reader))
    }
}

impl<S: Simd, Sampler: BufferSampler> SIMDSampleGrabber<S> for SIMDSampleGrabbers<S, Sampler> {
    #[inline(always)]
    fn get(&self, indexes: S::Vi32, fractional: S::Vf32) -> S::Vf32 {
        match self {
            SIMDSampleGrabbers::Linear(grabber) => grabber.get(indexes, fractional),
            SIMDSampleGrabbers::Nearest(grabber) => grabber.get(indexes, fractional),
        }
    }

    #[inline(always)]
    fn is_past_end(&self, pos: f64) -> bool {
        match self {
            SIMDSampleGrabbers::Linear(grabber) => grabber.is_past_end(pos),
            SIMDSampleGrabbers::Nearest(grabber) => grabber.is_past_end(pos),
        }
    }
}

// Sampler generator

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

    fn increment_time(&mut self, by: f64) -> f64 {
        let time = self.time;
        self.time += by;
        time
    }
}

impl<S, Pitch, Grabber> VoiceGeneratorBase for SIMDStereoVoiceSampler<S, Pitch, Grabber>
where
    S: Simd,
    Pitch: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
    Grabber: SIMDSampleGrabber<S>,
{
    fn ended(&self) -> bool {
        self.grabber_left.is_past_end(self.time) || self.grabber_right.is_past_end(self.time)
    }

    fn signal_release(&mut self) {
        self.pitch_gen.signal_release();
    }

    fn process_controls(&mut self, control: &crate::core::VoiceControlData) {
        self.pitch_gen.process_controls(control);
    }
}

impl<S, Pitch, Grabber> SIMDVoiceGenerator<S, SIMDSampleStereo<S>>
    for SIMDStereoVoiceSampler<S, Pitch, Grabber>
where
    S: Simd,
    Pitch: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
    Grabber: SIMDSampleGrabber<S>,
{
    fn next_sample(&mut self) -> SIMDSampleStereo<S> {
        let speed = self.pitch_gen.next_sample().0;
        let mut indexes = unsafe { S::set1_epi32(0) };
        let mut fractionals = unsafe { S::set1_ps(0.0) };

        for i in 0..S::VF32_WIDTH {
            let time = self.increment_time(speed[i] as f64);
            indexes[i] = time as i32;
            fractionals[i] = (time % 1.0) as f32;
        }

        let left = self.grabber_left.get(indexes, fractionals);
        let right = self.grabber_right.get(indexes, fractionals);

        SIMDSampleStereo(left, right)
    }
}
