use std::{f32::consts::PI, fs::File, io, path::PathBuf, sync::Arc};

use wav::BitDepth;

use super::AudioFileLoader;

fn resample(x: f32, indata: &[f32], fmax: f32, fsr: f32, wnwidth: i32) -> f32
{
    let r_g = 2.0 * fmax / fsr;
    let mut r_y = 0.0;
    for i in (-wnwidth / 2)..(wnwidth / 2 - 1) {
        let j = x as i32 + i;
        let r_w = 0.5 - 0.5 * (2.0 * PI * (0.5 + (j as f32 - x) / wnwidth as f32)).cos();
        let r_a = 2.0 * PI * (j as f32 - x) * fmax / fsr;
        let r_snc = if r_a != 0.0 { (r_a).sin() / r_a } else { 1.0 };
        if j >= 0 && j < indata.len() as i32 {
            r_y += r_g * r_w * r_snc * indata[j as usize];
        }
    }
    r_y
}

fn resample_vec(input: &[f32], in_rate: u32, out_rate: u32) -> Vec<f32>
{
    let in_rate = in_rate as f32;
    let out_rate = out_rate as f32;
    let mut output = vec![];
    let new_length = (input.len() as f32 / in_rate * out_rate) as usize;

    for i in 0..new_length {
        let pos = i as f32 / out_rate * in_rate;
        let input = resample(pos, &input, 10000.0, in_rate, 32);
        output.push(input);
    }

    output
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

        Ok(vecs.into_iter().map(|samples| resample_vec(&samples, header.sampling_rate, 96000).into()).collect())
    }
}
