use super::FilterBase;
use std::f32::consts::{PI, SQRT_2};

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
        let f = (freq * PI / sample_rate).tan();
        let a0r = 1.0 / (1.0 + SQRT_2 * f + f * f);
        let a1 = (2.0 * f * f - 2.0) * a0r;
        let a2 = (1.0 - SQRT_2 * f + f * f) * a0r;
        let b0 = f * f * a0r;
        let b1 = 2.0 * b0;
        let b2 = b0;
        Coefficients { a1, a2, b0, b1, b2 }
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
        let x0 = input;
        let y0 = self.coefs.b0 * x0 + self.coefs.b1 * self.x1 + self.coefs.b2 * self.x2
            - self.coefs.a1 * self.y1
            - self.coefs.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x0;
        self.y2 = self.y1;
        self.y1 = y0;
        y0
    }
}

