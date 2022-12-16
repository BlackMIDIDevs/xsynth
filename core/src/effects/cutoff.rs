use simdeez::Simd;

// IMPORTANT: The copy trait is necessary on these filters so that it can be initialized into a constant vector.

pub trait FilterBase: Default + Clone + Copy {
    fn calculate_alpha(cutoff: f32, sample_rate: f32) -> f32;

    fn process_sample(&mut self, alpha: f32, val: f32) -> f32;

    fn process_sample_simd<S: Simd>(&mut self, alpha: f32, val: S::Vf32) -> S::Vf32 {
        let mut out = val;
        for i in 0..S::VF32_WIDTH {
            out[i] = self.process_sample(alpha, val[i]);
        }
        out
    }
}

#[derive(Debug, Clone, Default, Copy)]
pub struct Lowpass {
    previous: f32,
}

impl FilterBase for Lowpass {
    fn calculate_alpha(cutoff: f32, sample_rate: f32) -> f32 {
        let rc = 1.0 / (cutoff * 2.0 * core::f32::consts::PI);
        let dt = 1.0 / sample_rate;
        dt / (rc + dt)
    }

    fn process_sample(&mut self, alpha: f32, val: f32) -> f32 {
        let out = alpha * val + (1.0 - alpha) * self.previous;
        self.previous = out;
        out
    }
}

#[derive(Debug, Clone, Default, Copy)]
pub struct Highpass {
    previous: f32,
    previous_unedited: f32,
}

impl FilterBase for Highpass {
    fn calculate_alpha(cutoff: f32, sample_rate: f32) -> f32 {
        let rc = 1.0 / (cutoff * 2.0 * core::f32::consts::PI);
        let dt = 1.0 / sample_rate;
        rc / (rc + dt)
    }

    fn process_sample(&mut self, alpha: f32, val: f32) -> f32 {
        let previous = val;
        let out = alpha * (self.previous + val - self.previous_unedited);
        self.previous = out;
        self.previous_unedited = previous;
        out
    }
}

pub trait CutoffFilterBase: Clone {
    fn new(cutoff: f32, sample_rate: f32) -> Self;
    fn set_cutoff(&mut self, cutoff: f32);
    fn process_sample(&mut self, val: f32) -> f32;
    fn process_sample_simd<S: Simd>(&mut self, val: S::Vf32) -> S::Vf32;
}

#[derive(Clone)]
pub struct MultiPassCutoff<F: FilterBase, const N: usize> {
    filter: [F; N],
    alpha: f32,
    sample_rate: f32,
}

impl<F: FilterBase, const N: usize> MultiPassCutoff<F, N> {
    pub fn new(cutoff: f32, sample_rate: f32) -> Self {
        Self {
            filter: [Default::default(); N],
            alpha: F::calculate_alpha(cutoff, sample_rate),
            sample_rate,
        }
    }

    pub fn set_cutoff(&mut self, cutoff: f32) {
        self.alpha = F::calculate_alpha(cutoff, self.sample_rate);
    }

    pub fn process_sample(&mut self, mut val: f32) -> f32 {
        for pass in 0..(N) {
            val = self.filter[pass].process_sample(self.alpha, val)
        }
        val
    }

    pub fn process_sample_simd<S: Simd>(&mut self, mut val: S::Vf32) -> S::Vf32 {
        for pass in 0..(N) {
            val = self.filter[pass].process_sample_simd::<S>(self.alpha, val)
        }
        val
    }
}

impl<F: FilterBase, const N: usize> CutoffFilterBase for MultiPassCutoff<F, N> {
    fn new(cutoff: f32, sample_rate: f32) -> Self {
        Self::new(cutoff, sample_rate)
    }

    fn set_cutoff(&mut self, cutoff: f32) {
        self.set_cutoff(cutoff);
    }

    fn process_sample(&mut self, val: f32) -> f32 {
        self.process_sample(val)
    }

    fn process_sample_simd<S: Simd>(&mut self, val: S::Vf32) -> S::Vf32 {
        self.process_sample_simd::<S>(val)
    }
}

pub struct MultiChannelCutoff<F: CutoffFilterBase> {
    channels: Vec<F>,
}

impl<F: CutoffFilterBase> MultiChannelCutoff<F> {
    pub fn new(channels: usize, cutoff: f32, sample_rate: f32) -> Self {
        Self {
            channels: (0..channels).map(|_| F::new(cutoff, sample_rate)).collect(),
        }
    }

    pub fn set_cutoff(&mut self, cutoff: f32) {
        for filter in self.channels.iter_mut() {
            filter.set_cutoff(cutoff);
        }
    }

    pub fn cutoff(&mut self, sample: &mut [f32]) {
        let channel_count = self.channels.len();
        for (i, s) in sample.iter_mut().enumerate() {
            *s = self.channels[i % channel_count].process_sample(*s);
        }
    }
}
