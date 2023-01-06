use super::FilterBase;
use std::f32::consts::{PI, FRAC_1_SQRT_2};

/// 2-Pole Butterworth Low Pass Filter

#[derive(Debug, Clone, Default, Copy)]
struct Coefficients {
    pub a1: f32,
    pub a2: f32,
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
}

#[derive(Debug, Clone, Default, Copy)]
pub struct ButterworthFilter {
    coefs: Coefficients,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
    sample_rate: f32,
}

impl ButterworthFilter {
    fn get_coefficients(sample_rate: f32, freq: f32) -> Coefficients {
        let omega = 2.0 * PI * freq / sample_rate;

        let omega_s = omega.sin();
        let omega_c = omega.cos();
        let alpha = omega_s / (2.0 * FRAC_1_SQRT_2);

        let b0 = (1.0 - omega_c) * 0.5;
        let b1 = 1.0 - omega_c;
        let b2 = (1.0 - omega_c) * 0.5;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * omega_c;
        let a2 = 1.0 - alpha;

        Coefficients {
            a1: a1 / a0,
            a2: a2 / a0,
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
        }
    }
}

impl FilterBase for ButterworthFilter {
    fn new(sample_rate: f32, freq: f32) -> Self {
        Self {
            coefs: Self::get_coefficients(sample_rate, freq),
            x1: 0.,
            x2: 0.,
            y1: 0.,
            y2: 0.,
            sample_rate,
        }
    }

    fn set_frequency(&mut self, freq: f32) {
        self.coefs = Self::get_coefficients(self.sample_rate, freq);
    }

    fn tick(&mut self, input: f32) -> f32 {
        let out = self.coefs.b0 * input + self.coefs.b1 * self.x1 + self.coefs.b2 * self.x2
            - self.coefs.a1 * self.y1
            - self.coefs.a2 * self.y2;

        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = out;

        out
    }
}

