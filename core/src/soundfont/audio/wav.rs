use std::{f32::consts::PI, fs::File, io, path::PathBuf, sync::Arc};

use wav::BitDepth;

use super::AudioFileLoader;

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

struct SincResampler {
    sample_rate: u32,
    resolution: f32,
    offset: i32,
    stride: usize,
    lookup_table: Vec<f32>,
}

impl SincResampler {
    fn new(resolution: usize, sample_rate: u32, wnwidth: i32) -> Self {
        let lookup_table =
            gen_resample_lookup_table(resolution, 20000.0, sample_rate as f32, wnwidth);
        SincResampler {
            sample_rate,
            resolution: resolution as f32,
            offset: wnwidth / 2,
            stride: ((wnwidth / 2 - 1) - (-wnwidth / 2)) as usize,
            lookup_table,
        }
    }

    fn resample_vec(&self, indata: &[f32], sample_rate: u32) -> Vec<f32> {
        let new_len = indata.len() * sample_rate as usize / self.sample_rate as usize;
        let mut outdata = Vec::with_capacity(new_len);

        let rate_fac = self.sample_rate as f32 / sample_rate as f32;
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

impl AudioFileLoader {
    pub fn load_wav(path: &PathBuf) -> io::Result<Vec<Arc<[f32]>>> {
        let mut reader = File::open(path)?;
        let (header, data) = wav::read(&mut reader)?;

        fn build_arrays<T: Copy, F: Fn(T) -> f32>(
            arr: &[T],
            channels: u16,
            cast: F,
        ) -> Vec<Vec<f32>> {
            let mut chans = Vec::new();
            for _ in 0..channels {
                chans.push(Vec::new());
            }

            for i in 0..arr.len() {
                let v = cast(arr[i]);
                chans[i % channels as usize].push(v);
            }

            for chan in chans.iter_mut() {
                chan.shrink_to_fit();
            }

            chans
        }

        fn extract_samples(data: BitDepth, channels: u16) -> Vec<Vec<f32>> {
            match data.as_eight() {
                Some(data) => return build_arrays(data, channels, |v| (v as f32 - 128.0) / 128.0),
                None => {}
            };

            match data.as_sixteen() {
                Some(data) => {
                    return build_arrays(data, channels, |v| (v as f32) / i16::MAX as f32)
                }
                None => {}
            };

            match data.as_thirty_two_float() {
                Some(data) => return build_arrays(data, channels, |v| v),
                None => {}
            };

            match data.as_twenty_four() {
                Some(data) => return build_arrays(data, channels, |v| v as f32 / (1 << 23) as f32),
                None => {}
            }

            panic!()
        }

        let vecs = extract_samples(data, header.channel_count);

        let resampler = SincResampler::new(10000, header.sampling_rate, 32);

        Ok(vecs
            .into_iter()
            .map(|samples| resampler.resample_vec(&samples, 96000).into())
            // .map(|samples| resample_vec(&samples, header.sampling_rate, 96000).into())
            .collect())
    }
}
