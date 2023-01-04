mod butterworth;
pub use butterworth::*;
mod lowpole;
pub use lowpole::*;
mod highpole;
pub use highpole::*;

use simdeez::Simd;

// IMPORTANT: The copy trait is necessary on these filters so that it can be initialized into a constant vector.

pub trait FilterBase: Default + Clone + Copy {
    fn new(freq: f32, sample_rate: f32) -> Self;

    fn set_frequency(&mut self, freq: f32);

    fn tick(&mut self, val: f32) -> f32;

    fn process_simd<S: Simd>(&mut self, val: S::Vf32) -> S::Vf32 {
        let mut out = val;
        for i in 0..S::VF32_WIDTH {
            out[i] = self.tick(val[i]);
        }
        out
    }
}

pub struct MultiChannelPole<F: FilterBase> {
    channels: Vec<F>,
}

impl<F: FilterBase> MultiChannelPole<F> {
    pub fn new(channels: usize, freq: f32, sample_rate: f32) -> Self {
        Self {
            channels: (0..channels).map(|_| F::new(freq, sample_rate)).collect(),
        }
    }

    pub fn set_frequency(&mut self, freq: f32) {
        for filter in self.channels.iter_mut() {
            filter.set_frequency(freq);
        }
    }

    pub fn process(&mut self, sample: &mut [f32]) {
        let channel_count = self.channels.len();
        for (i, s) in sample.iter_mut().enumerate() {
            *s = self.channels[i % channel_count].tick(*s);
        }
    }
}
