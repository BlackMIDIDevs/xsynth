use simdeez::Simd;
use soundfonts::FilterType;

use simdeez::Simd;

pub struct SingleChannelFilter {
    filter_type: FilterType,
    previous: Vec<f32>,
    previous_unedited: Vec<f32>,
    alpha: f32,
    sample_rate: f32,
}

impl SingleChannelFilter {
    pub fn new(filter_type: FilterType, cutoff: f32, sample_rate: f32) -> Self {
        let alpha = Self::calculate_alpha(filter_type, cutoff, sample_rate);

        let passes = match filter_type {
            FilterType::LowPass { passes } => passes,
            FilterType::HighPass { passes } => passes,
        };

        Self {
            filter_type,
            previous: vec![0.0; passes],
            previous_unedited: vec![0.0; passes],
            alpha,
            sample_rate,
        }
    }

    pub fn set_cutoff(&mut self, cutoff: f32) {
        self.alpha = Self::calculate_alpha(self.filter_type, cutoff, self.sample_rate);
    }

    fn calculate_alpha(filter_type: FilterType, cutoff: f32, sample_rate: f32) -> f32 {
        let rc = 1.0 / (cutoff * 2.0 * core::f32::consts::PI);
        let dt = 1.0 / sample_rate;

        match filter_type {
            FilterType::LowPass { .. } => dt / (rc + dt),
            FilterType::HighPass { .. } => rc / (rc + dt),
        }
    }

    pub fn process_sample(&mut self, val: f32) -> f32 {
        let mut out = val;
        match self.filter_type {
            FilterType::LowPass { passes } => {
                for pass in 0..passes {
                    out = self.alpha * out + (1.0 - self.alpha) * self.previous[pass];
                    self.previous[pass] = out;
                }
            }
            FilterType::HighPass { passes } => {
                for pass in 0..passes {
                    let previous = out;
                    out = self.alpha * (self.previous[pass] + out - self.previous_unedited[pass]);
                    self.previous[pass] = out;
                    self.previous_unedited[pass] = previous;
                }
            }
        }
        out
    }

    pub fn process_sample_simd<S: Simd>(&mut self, val: S::Vf32) -> S::Vf32 {
        let mut out = val;
        match self.filter_type {
            FilterType::LowPass { passes } => {
                for pass in 0..passes {
                    for i in 0..S::VF32_WIDTH {
                        out[i] = self.alpha * out[i] + (1.0 - self.alpha) * self.previous[pass];
                        self.previous[pass] = out[i];
                    }
                }
            }
            FilterType::HighPass { passes } => {
                for pass in 0..passes {
                    for i in 0..S::VF32_WIDTH {
                        let previous = out[i];
                        out[i] = self.alpha
                            * (self.previous[pass] + out[i] - self.previous_unedited[pass]);
                        self.previous[pass] = out[i];
                        self.previous_unedited[pass] = previous;
                    }
                }
            }
        }
        out
    }
}

pub struct AudioFilter {
    channels: Vec<SingleChannelFilter>,
    passes: usize,
    channel_count: usize,
}

impl AudioFilter {
    pub fn new(filter_type: FilterType, channel_count: u16, cutoff: f32, sample_rate: f32) -> Self {
        let mut limiters = Vec::new();
        let passes = match filter_type {
            FilterType::LowPass { passes } => passes,
            FilterType::HighPass { passes } => passes,
        };
        for _ in 0..channel_count * passes as u16 {
            limiters.push(SingleChannelFilter::new(filter_type, cutoff, sample_rate));
        }
        Self {
            channels: limiters,
            passes,
            channel_count: channel_count as usize,
        }
    }

    pub fn process_samples(&mut self, sample: &mut [f32]) {
        for p in 0..self.passes {
            for (i, s) in sample.iter_mut().enumerate() {
                *s = self.channels[p + i % self.channel_count].process_sample(*s);
            }
        }
    }

    pub fn process_samples_simd<S: Simd>(&mut self, sample: &mut <S as Simd>::Vf32) {
        // Only mono
        for p in 0..self.passes {
            for i in 0..S::VF32_WIDTH {
                sample[i] = self.channels[p].process_sample(sample[i]);
            }
        }
    }

    pub fn set_cutoff(&mut self, cutoff: f32) {
        for channel in self.channels.iter_mut() {
            channel.set_cutoff(cutoff);
        }
    }
}
