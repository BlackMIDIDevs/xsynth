use soundfonts::FilterType;

use simdeez::Simd;

pub struct SingleChannelFilter {
    filter_type: FilterType,
    previous: f32,
    previous_unedited: f32,
    alpha: f32,
    sample_rate: f32,
}

impl SingleChannelFilter {
    pub fn new(filter_type: FilterType, cutoff: f32, sample_rate: f32) -> Self {
        let alpha = Self::calculate_alpha(filter_type, cutoff, sample_rate);

        Self {
            filter_type,
            previous: 0.0,
            previous_unedited: 0.0,
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
            FilterType::LowPass { .. } => {
                out = self.alpha * out + (1.0 - self.alpha) * self.previous;
                self.previous = out;
            }
            FilterType::HighPass { .. } => {
                out = self.alpha * (self.previous + out - self.previous_unedited);
                self.previous = out;
                self.previous_unedited = val;
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
