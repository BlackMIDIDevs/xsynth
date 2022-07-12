use std::{f32::consts::PI, fs::File, io, path::PathBuf, sync::Arc};

use flac::{ErrorKind, StreamReader};
use thiserror::Error;
use wav::BitDepth;

fn gen_resample_lookup_table(resolution: usize, fmax: f32, fsr: f32, wnwidth: i32) -> Vec<f32> {
    let r_g = 2.0 * fmax / fsr;
    let mut lookup_table = Vec::new();
    for x in 0..resolution {
        let x = x as f32 / resolution as f32;
        for i in (-wnwidth / 2)..(wnwidth / 2 - 1) {
            let j_x = i as f32 - x;
            let r_a = 2.0 * PI * j_x * fmax / fsr;
            let r_w = 0.5 - 0.5 * (2.0 * PI * (0.5 + j_x / wnwidth as f32)).cos();
            let r_snc = if r_a != 0.0 { (r_a).sin() / r_a } else { 1.0 };
            lookup_table.push(r_g * r_w * r_snc);
        }
    }
    lookup_table
}

pub struct SincResampler {
    sample_rate: f32,
    resolution: f32,
    offset: i32,
    stride: usize,
    lookup_table: Vec<f32>,
}

impl SincResampler {
    pub fn new(resolution: usize, sample_rate: f32, wnwidth: i32) -> Self {
        let lookup_table = gen_resample_lookup_table(resolution, 20000.0, sample_rate, wnwidth);
        SincResampler {
            sample_rate,
            resolution: resolution as f32,
            offset: wnwidth / 2,
            stride: ((wnwidth / 2 - 1) - (-wnwidth / 2)) as usize,
            lookup_table,
        }
    }

    pub fn resample_vec(&self, indata: &[f32], sample_rate: f32) -> Vec<f32> {
        let new_len = indata.len() * sample_rate as usize / self.sample_rate as usize;
        let mut outdata = Vec::with_capacity(new_len);

        let rate_fac = self.sample_rate as f32 / sample_rate;
        for s in 0..new_len {
            let x = s as f32 * rate_fac;

            let mut r_y = 0.0;
            for p in 0..self.stride {
                let i = p as i32 - self.offset;
                let j = x as i32 + i;

                if j >= 0 && j < indata.len() as i32 {
                    let res_index = ((x % 1.0) * self.resolution) as usize;
                    let index = res_index * self.stride + p;
                    r_y += self.lookup_table[index] * indata[j as usize];
                }
            }
            outdata.push(r_y);
        }

        outdata
    }
}
