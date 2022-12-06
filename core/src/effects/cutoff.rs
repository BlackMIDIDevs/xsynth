pub struct SingleChannelCutoff {
    previous: f32,
    alpha: f32,
    sample_rate: f32,
}

impl SingleChannelCutoff {
    pub fn new(cutoff: f32, sample_rate: f32) -> SingleChannelCutoff {
        let alpha = Self::calculate_alpha(cutoff, sample_rate);

        SingleChannelCutoff {
            previous: 0.0,
            alpha,
            sample_rate,
        }
    }

    pub fn set_cutoff(&mut self, cutoff: f32) {
        self.alpha = Self::calculate_alpha(cutoff, self.sample_rate);
    }

    fn calculate_alpha(cutoff: f32, sample_rate: f32) -> f32 {
        let rc = 1.0 / (cutoff * 2.0 * core::f32::consts::PI);
        let dt = 1.0 / sample_rate;
        let alpha = dt / (rc + dt);
        alpha
    }

    pub fn cutoff_sample(&mut self, val: f32) -> f32 {
        let val = self.alpha * val + (1.0 - self.alpha) * self.previous;
        self.previous = val;
        val
    }
}

pub struct CutoffFilter {
    channels: Vec<SingleChannelCutoff>,
    channel_count: usize,
}

impl CutoffFilter {
    pub fn new(channel_count: u16, cutoff: f32, sample_rate: f32) -> CutoffFilter {
        let mut limiters = Vec::new();
        for _ in 0..channel_count {
            limiters.push(SingleChannelCutoff::new(cutoff, sample_rate));
        }
        CutoffFilter {
            channels: limiters,
            channel_count: channel_count as usize,
        }
    }

    pub fn cutoff_samples(&mut self, sample: &mut [f32]) {
        for (i, s) in sample.iter_mut().enumerate() {
            *s = self.channels[i % self.channel_count].cutoff_sample(*s);
        }
    }

    pub fn set_cutoff(&mut self, cutoff: f32) {
        for channel in self.channels.iter_mut() {
            channel.set_cutoff(cutoff);
        }
    }
}
