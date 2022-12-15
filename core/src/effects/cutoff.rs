use simdeez::Simd;
use soundfonts::FilterType;

pub struct LowpassFilterBase {
    alpha: f32,
    sample_rate: f32,
    previous: Vec<f32>,
}

impl LowpassFilterBase {
    pub fn new(passes: usize, cutoff: f32, sample_rate: f32) -> Self {
        let alpha = Self::calculate_alpha(cutoff, sample_rate);

        Self {
            previous: vec![0.0; passes],
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
        dt / (rc + dt)
    }

    pub fn process_sample(&mut self, val: f32) -> f32 {
        let mut out = val;
        for pass in 0..self.previous.len() {
            out = self.alpha * out + (1.0 - self.alpha) * self.previous[pass];
            self.previous[pass] = out;
        }
        out
    }

    pub fn process_sample_simd<S: Simd>(&mut self, val: S::Vf32) -> S::Vf32 {
        let mut out = val;
        for pass in 0..self.previous.len() {
            for i in 0..S::VF32_WIDTH {
                out[i] = self.alpha * out[i] + (1.0 - self.alpha) * self.previous[pass];
                self.previous[pass] = out[i];
            }
        }
        out
    }
}

pub struct HighpassFilterBase {
    alpha: f32,
    sample_rate: f32,
    previous: Vec<f32>,
    previous_unedited: Vec<f32>,
}

impl HighpassFilterBase {
    pub fn new(passes: usize, cutoff: f32, sample_rate: f32) -> Self {
        let alpha = Self::calculate_alpha(cutoff, sample_rate);

        Self {
            previous_unedited: vec![0.0; passes],
            previous: vec![0.0; passes],
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
        rc / (rc + dt)
    }

    pub fn process_sample(&mut self, val: f32) -> f32 {
        let mut out = val;
        for pass in 0..self.previous.len() {
            let previous = out;
            out = self.alpha * (self.previous[pass] + out - self.previous_unedited[pass]);
            self.previous[pass] = out;
            self.previous_unedited[pass] = previous;
        }
        out
    }

    pub fn process_sample_simd<S: Simd>(&mut self, val: S::Vf32) -> S::Vf32 {
        let mut out = val;
        for pass in 0..self.previous.len() {
            for i in 0..S::VF32_WIDTH {
                let previous = out[i];
                out[i] = self.alpha * (self.previous[pass] + out[i] - self.previous_unedited[pass]);
                self.previous[pass] = out[i];
                self.previous_unedited[pass] = previous;
            }
        }
        out
    }
}

pub enum SingleChannelFilter {
    LowPass(LowpassFilterBase),
    HighPass(HighpassFilterBase),
}

impl SingleChannelFilter {
    pub fn new(filter_type: FilterType, cutoff: f32, sample_rate: f32) -> Self {
        match filter_type {
            FilterType::LowPass { passes } => {
                Self::LowPass(LowpassFilterBase::new(passes, cutoff, sample_rate))
            }
            FilterType::HighPass { passes } => {
                Self::HighPass(HighpassFilterBase::new(passes, cutoff, sample_rate))
            }
        }
    }

    pub fn set_cutoff(&mut self, cutoff: f32) {
        match self {
            Self::LowPass(filter) => filter.set_cutoff(cutoff),
            Self::HighPass(filter) => filter.set_cutoff(cutoff),
        }
    }

    pub fn process_sample(&mut self, val: f32) -> f32 {
        match self {
            Self::LowPass(filter) => filter.process_sample(val),
            Self::HighPass(filter) => filter.process_sample(val),
        }
    }

    pub fn process_sample_simd<S: Simd>(&mut self, val: S::Vf32) -> S::Vf32 {
        match self {
            Self::LowPass(filter) => filter.process_sample_simd::<S>(val),
            Self::HighPass(filter) => filter.process_sample_simd::<S>(val),
        }
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
