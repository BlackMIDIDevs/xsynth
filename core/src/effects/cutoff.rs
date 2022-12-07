use soundfonts::FilterType;

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
        let mut previous = Vec::new();
        for _ in 0..passes {
            previous.push(0.0);
        }
        let mut previous_unedited = Vec::new();
        for _ in 0..passes {
            previous_unedited.push(0.0);
        }

        Self {
            filter_type,
            previous,
            previous_unedited,
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
                for i in 0..passes {
                    out = self.alpha * out + (1.0 - self.alpha) * self.previous[i];
                    self.previous[i] = out;
                }
            },
            FilterType::HighPass { passes } => {
                for i in 0..passes {
                    out = self.alpha * (self.previous[i] + out - self.previous_unedited[i]);
                    self.previous[i] = out;
                }
                self.previous_unedited[0] = val;
                for i in 0..self.previous.len()-1 {
                    self.previous_unedited[i+1] = self.previous[i];
                }
            },
        }
        out
    }
}

pub struct AudioFilter {
    channels: Vec<SingleChannelFilter>,
    channel_count: usize,
}

impl AudioFilter {
    pub fn new(filter_type: FilterType, channel_count: u16, cutoff: f32, sample_rate: f32) -> Self {
        let mut limiters = Vec::new();
        for _ in 0..channel_count {
            limiters.push(SingleChannelFilter::new(filter_type, cutoff, sample_rate));
        }
        Self {
            channels: limiters,
            channel_count: channel_count as usize,
        }
    }

    pub fn process_samples(&mut self, sample: &mut [f32]) {
        for (i, s) in sample.iter_mut().enumerate() {
            *s = self.channels[i % self.channel_count].process_sample(*s);
        }
    }

    pub fn set_cutoff(&mut self, cutoff: f32) {
        for channel in self.channels.iter_mut() {
            channel.set_cutoff(cutoff);
        }
    }
}
