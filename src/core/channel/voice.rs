use std::f64::consts::PI;

use lazy_static::lazy_static;

pub struct Voice {
    freq: f64,

    amp: f32,
    phase: f64,

    _vel: u8,
}

fn build_frequencies() -> [f32; 128] {
    let mut freqs = [0.0f32; 128];
    for key in 0..freqs.len() {
        freqs[key] = 2.0f32.powf((key as f32 - 69.0) / 12.0) * 440.0;
    }
    freqs
}

lazy_static! {
    static ref FREQS: [f32; 128] = build_frequencies();
}

impl Voice {
    pub fn spawn(key: u8, vel: u8, sample_rate: u32) -> Voice {
        let freq = (FREQS[key as usize] as f64 / sample_rate as f64) * PI;
        let amp = 1.04f32.powf(vel as f32 - 127.0);

        Voice {
            freq,
            amp,
            phase: 0.0,
            _vel: vel,
        }
    }

    pub fn render_to(&mut self, out: &mut [f32]) {
        for i in 0..out.len() {
            let sample = self.amp * self.phase.cos() as f32;
            self.phase += self.freq;
            out[i] += sample;
        }
    }
}
