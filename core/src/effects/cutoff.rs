use simdeez::Simd;

pub trait FilterBase: Default + Clone {
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

#[derive(Debug, Clone, Default)]
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

#[derive(Debug, Clone, Default)]
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

macro_rules! make_pass_cutoff {
    ($name:ident, $passes:expr) => {
        #[derive(Clone)]
        pub struct $name<F: FilterBase> {
            filter: [F; $passes],
            alpha: f32,
            sample_rate: f32,
        }

        impl<F: FilterBase> $name<F> {
            pub fn new(cutoff: f32, sample_rate: f32) -> Self {
                Self {
                    filter: Default::default(),
                    alpha: F::calculate_alpha(cutoff, sample_rate),
                    sample_rate,
                }
            }

            pub fn set_cutoff(&mut self, cutoff: f32) {
                self.alpha = F::calculate_alpha(cutoff, self.sample_rate);
            }

            pub fn process_sample(&mut self, mut val: f32) -> f32 {
                for pass in 0..($passes) {
                    val = self.filter[pass].process_sample(self.alpha, val)
                }
                val
            }

            pub fn process_sample_simd<S: Simd>(&mut self, mut val: S::Vf32) -> S::Vf32 {
                for pass in 0..($passes) {
                    val = self.filter[pass].process_sample_simd::<S>(self.alpha, val)
                }
                val
            }
        }

        impl<F: FilterBase> CutoffFilterBase for $name<F> {
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
    };
}

make_pass_cutoff!(OnePassCutoff, 1);
make_pass_cutoff!(TwoPassCutoff, 2);
make_pass_cutoff!(FourPassCutoff, 4);
make_pass_cutoff!(SixPassCutoff, 6);

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
