use super::FilterBase;

/// 1-Pole Low Pass Filter

#[derive(Debug, Clone, Default, Copy)]
pub struct LowPole {
    a: f32,
    y1: f32,
    sample_rate: f32,
}

impl LowPole {
    fn get_coefficients(freq: f32, sample_rate: f32) -> f32 {
        let rc = 1.0 / (freq * 2.0 * core::f32::consts::PI);
        let dt = 1.0 / sample_rate;
        dt / (rc + dt)
    }
}

impl FilterBase for LowPole {
    fn new(freq: f32, sample_rate: f32) -> Self {
        Self {
            a: Self::get_coefficients(freq, sample_rate),
            y1: 0.,
            sample_rate,
        }
    }

    fn set_frequency(&mut self, freq: f32) {
        self.a = Self::get_coefficients(self.sample_rate, freq);
    }

    fn tick(&mut self, val: f32) -> f32 {
        let out = self.a * val + (1.0 - self.a) * self.y1;
        self.y1 = out;
        println!("{out}");
        out
    }
}
