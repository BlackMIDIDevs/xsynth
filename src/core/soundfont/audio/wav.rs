use std::{fs::File, io, path::PathBuf, sync::Arc};

use wav::BitDepth;

use super::AudioFileLoader;

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

        Ok(vecs.into_iter().map(|samples| samples.into()).collect())
    }
}
