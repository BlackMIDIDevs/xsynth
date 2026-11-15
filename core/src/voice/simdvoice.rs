use std::marker::PhantomData;

use simdeez::prelude::*;

use crate::voice::{ReleaseType, VoiceControlData};

use super::{
    SIMDSampleMono, SIMDSampleStereo, SIMDVoiceGenerator, VoiceGeneratorBase, VoiceSampleGenerator,
};

pub struct SIMDStereoVoice<S: Simd, T: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>> {
    generator: T,
    /// Unused part of the last generated vector, only allocated when a buffer
    /// ends inside a vector so that voices stay small.
    remainder: Option<Box<SIMDSampleStereo<S>>>,
    remainder_pos: u8,
    _s: PhantomData<S>,
}

impl<S: Simd, T: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>> SIMDStereoVoice<S, T> {
    pub fn new(generator: T) -> SIMDStereoVoice<S, T> {
        SIMDStereoVoice {
            generator,
            remainder: None,
            remainder_pos: S::Vf32::WIDTH as u8,
            _s: PhantomData,
        }
    }
}

impl<S, T> VoiceGeneratorBase for SIMDStereoVoice<S, T>
where
    S: Simd,
    T: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
{
    #[inline(always)]
    fn ended(&self) -> bool {
        self.generator.ended()
    }

    #[inline(always)]
    fn signal_release(&mut self, rel_type: ReleaseType) {
        self.generator.signal_release(rel_type)
    }

    #[inline(always)]
    fn process_controls(&mut self, control: &VoiceControlData) {
        self.generator.process_controls(control)
    }

    #[inline(always)]
    fn prefetch(&self, frames: usize) {
        self.generator.prefetch(frames)
    }
}

impl<S, T> VoiceSampleGenerator for SIMDStereoVoice<S, T>
where
    S: Simd,
    T: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
{
    fn render_to(&mut self, buffer: &mut [f32]) {
        simd_block!(S, {
            let width = S::Vf32::WIDTH;
            let mut frames = buffer.as_chunks_mut::<2>().0;

            // Finish the vector left over from the previous call.
            if (self.remainder_pos as usize) < width {
                let remainder = self.remainder.as_deref().unwrap();
                while (self.remainder_pos as usize) < width {
                    let Some((frame, rest)) = frames.split_first_mut() else {
                        return;
                    };
                    frame[0] += remainder.0[self.remainder_pos as usize];
                    frame[1] += remainder.1[self.remainder_pos as usize];
                    self.remainder_pos += 1;
                    frames = rest;
                }
            }

            // Whole vectors go straight to the output.
            let mut chunks = frames.chunks_exact_mut(width);
            for chunk in &mut chunks {
                let sample = self.generator.next_sample();
                for (i, frame) in chunk.iter_mut().enumerate() {
                    frame[0] += sample.0[i];
                    frame[1] += sample.1[i];
                }
            }

            // Keep the unused part of the last vector for the next call.
            let rest = chunks.into_remainder();
            if !rest.is_empty() {
                let sample = self.generator.next_sample();
                for (i, frame) in rest.iter_mut().enumerate() {
                    frame[0] += sample.0[i];
                    frame[1] += sample.1[i];
                }
                match self.remainder.as_deref_mut() {
                    Some(remainder) => *remainder = sample,
                    None => self.remainder = Some(Box::new(sample)),
                }
                self.remainder_pos = rest.len() as u8;
            }
        })
    }

    fn render_planar(&mut self, left: &mut [f32], right: &mut [f32]) {
        simd_block!(S, {
            let width = S::Vf32::WIDTH;
            let len = left.len().min(right.len());
            let (left, right) = (&mut left[..len], &mut right[..len]);
            let mut i = 0;

            // Finish the vector left over from the previous call.
            if (self.remainder_pos as usize) < width {
                let remainder = self.remainder.as_deref().unwrap();
                while (self.remainder_pos as usize) < width && i < len {
                    left[i] += remainder.0[self.remainder_pos as usize];
                    right[i] += remainder.1[self.remainder_pos as usize];
                    self.remainder_pos += 1;
                    i += 1;
                }
                if i == len {
                    return;
                }
            }

            // Whole vectors are added to the channel buffers directly.
            while i + width <= len {
                let sample = self.generator.next_sample();
                let l = &mut left[i..i + width];
                (S::Vf32::load_from_slice(l) + sample.0).copy_to_slice(l);
                let r = &mut right[i..i + width];
                (S::Vf32::load_from_slice(r) + sample.1).copy_to_slice(r);
                i += width;
            }

            // Keep the unused part of the last vector for the next call.
            if i < len {
                let sample = self.generator.next_sample();
                for j in 0..len - i {
                    left[i + j] += sample.0[j];
                    right[i + j] += sample.1[j];
                }
                match self.remainder.as_deref_mut() {
                    Some(remainder) => *remainder = sample,
                    None => self.remainder = Some(Box::new(sample)),
                }
                self.remainder_pos = (len - i) as u8;
            }
        })
    }
}

pub struct SIMDMonoVoice<S: Simd, T: SIMDVoiceGenerator<S, SIMDSampleMono<S>>> {
    generator: T,
    /// Unused part of the last generated vector, only allocated when a buffer
    /// ends inside a vector so that voices stay small.
    remainder: Option<Box<SIMDSampleMono<S>>>,
    remainder_pos: u8,
    _s: PhantomData<S>,
}

impl<S: Simd, T: SIMDVoiceGenerator<S, SIMDSampleMono<S>>> SIMDMonoVoice<S, T> {
    pub fn new(generator: T) -> SIMDMonoVoice<S, T> {
        SIMDMonoVoice {
            generator,
            remainder: None,
            remainder_pos: S::Vf32::WIDTH as u8,
            _s: PhantomData,
        }
    }
}

impl<S, T> VoiceGeneratorBase for SIMDMonoVoice<S, T>
where
    S: Simd,
    T: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
{
    #[inline(always)]
    fn ended(&self) -> bool {
        self.generator.ended()
    }

    #[inline(always)]
    fn signal_release(&mut self, rel_type: ReleaseType) {
        self.generator.signal_release(rel_type)
    }

    #[inline(always)]
    fn process_controls(&mut self, control: &VoiceControlData) {
        self.generator.process_controls(control)
    }

    #[inline(always)]
    fn prefetch(&self, frames: usize) {
        self.generator.prefetch(frames)
    }
}

impl<S, T> VoiceSampleGenerator for SIMDMonoVoice<S, T>
where
    S: Simd,
    T: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
{
    fn render_to(&mut self, buffer: &mut [f32]) {
        simd_block!(S, {
            let width = S::Vf32::WIDTH;
            let mut samples = buffer;

            // Finish the vector left over from the previous call.
            if (self.remainder_pos as usize) < width {
                let remainder = self.remainder.as_deref().unwrap();
                while (self.remainder_pos as usize) < width {
                    let Some((sample, rest)) = samples.split_first_mut() else {
                        return;
                    };
                    *sample += remainder.0[self.remainder_pos as usize];
                    self.remainder_pos += 1;
                    samples = rest;
                }
            }

            // Whole vectors go straight to the output.
            let mut chunks = samples.chunks_exact_mut(width);
            for chunk in &mut chunks {
                let value = self.generator.next_sample().0;
                let sum = S::Vf32::load_from_slice(chunk) + value;
                sum.copy_to_slice(chunk);
            }

            // Keep the unused part of the last vector for the next call.
            let rest = chunks.into_remainder();
            if !rest.is_empty() {
                let value = self.generator.next_sample();
                for (i, sample) in rest.iter_mut().enumerate() {
                    *sample += value.0[i];
                }
                match self.remainder.as_deref_mut() {
                    Some(remainder) => *remainder = value,
                    None => self.remainder = Some(Box::new(value)),
                }
                self.remainder_pos = rest.len() as u8;
            }
        })
    }

    fn render_planar(&mut self, left: &mut [f32], _right: &mut [f32]) {
        self.render_to(left)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use simdeez::simd_runtime_generate;

    /// Generates the sample index as the sample value.
    struct Counter(f32);

    impl VoiceGeneratorBase for Counter {
        fn ended(&self) -> bool {
            false
        }
        fn signal_release(&mut self, _: ReleaseType) {}
        fn process_controls(&mut self, _: &VoiceControlData) {}
    }

    impl<S: Simd> SIMDVoiceGenerator<S, SIMDSampleStereo<S>> for Counter {
        fn next_sample(&mut self) -> SIMDSampleStereo<S> {
            let mut v = S::Vf32::zeroes();
            for i in 0..S::Vf32::WIDTH {
                v[i] = self.0;
                self.0 += 1.0;
            }
            SIMDSampleStereo(v, v * S::Vf32::set1(-1.0))
        }
    }

    impl<S: Simd> SIMDVoiceGenerator<S, SIMDSampleMono<S>> for Counter {
        fn next_sample(&mut self) -> SIMDSampleMono<S> {
            let mut v = S::Vf32::zeroes();
            for i in 0..S::Vf32::WIDTH {
                v[i] = self.0;
                self.0 += 1.0;
            }
            SIMDSampleMono(v)
        }
    }

    #[test]
    fn split_buffers_match_one_render() {
        simd_runtime_generate!(
            fn run() {
                let sizes = [3usize, 1, 8, 13, 5, 16, 2, 7, 33];
                let total: usize = sizes.iter().sum();

                let mut stereo = SIMDStereoVoice::<S, _>::new(Counter(0.0));
                let mut split = vec![0.0f32; total * 2];
                let mut rest = &mut split[..];
                for size in sizes {
                    let (chunk, tail) = rest.split_at_mut(size * 2);
                    stereo.render_to(chunk);
                    rest = tail;
                }
                for (i, frame) in split.chunks(2).enumerate() {
                    assert_eq!(frame, [i as f32, -(i as f32)]);
                }

                let mut mono = SIMDMonoVoice::<S, _>::new(Counter(0.0));
                let mut split = vec![0.0f32; total];
                let mut rest = &mut split[..];
                for size in sizes {
                    let (chunk, tail) = rest.split_at_mut(size);
                    mono.render_to(chunk);
                    rest = tail;
                }
                for (i, v) in split.iter().enumerate() {
                    assert_eq!(*v, i as f32);
                }
            }
        );
        run();
    }
}
