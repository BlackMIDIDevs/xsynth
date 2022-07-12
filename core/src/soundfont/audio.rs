use std::{path::PathBuf, sync::Arc, io, fs::File};

use flac::{StreamReader, ErrorKind};
use thiserror::Error;
use wav::BitDepth;

use self::resample::SincResampler;

pub mod resample;

fn build_arrays<T: Copy>(
    iter: impl Iterator<Item = T>,
    channels: u16,
    cast: impl Fn(T) -> f32,
) -> Vec<Vec<f32>> {
    let mut chans = Vec::new();
    for _ in 0..channels {
        chans.push(Vec::new());
    }

    for (i, sample) in iter.enumerate() {
        let v = cast(sample);
        chans[i % channels as usize].push(v);
    }

    for chan in chans.iter_mut() {
        chan.shrink_to_fit();
    }

    chans
}

fn extract_samples(data: BitDepth, channels: u16) -> Vec<Vec<f32>> {
    match data.as_eight() {
        Some(data) => {
            return build_arrays(data.iter().copied(), channels, |v| {
                (v as f32 - 128.0) / 128.0
            })
        }
        None => {}
    };

    match data.as_sixteen() {
        Some(data) => {
            return build_arrays(data.iter().copied(), channels, |v| {
                (v as f32) / i16::MAX as f32
            })
        }
        None => {}
    };

    match data.as_thirty_two_float() {
        Some(data) => return build_arrays(data.iter().copied(), channels, |v| v),
        None => {}
    };

    match data.as_twenty_four() {
        Some(data) => {
            return build_arrays(data.iter().copied(), channels, |v| {
                v as f32 / (1 << 23) as f32
            })
        }
        None => {}
    }

    panic!()
}

#[derive(Debug, Error)]
pub enum AudioLoadError {
    #[error("IO Error")]
    IOError(#[from] io::Error),

    #[error("Unknown audio sample file extension")]
    UnknownExtension,

    #[error("Error parsing FLAC file")]
    FlacParseError,
}

fn resample_vecs(vecs: Vec<Vec<f32>>, sample_rate: f32, new_sample_rate: f32) -> Vec<Arc<[f32]>> {
    let resampler = SincResampler::new(10000, sample_rate, 32);

    vecs.into_iter()
        .map(|samples| resampler.resample_vec(&samples, new_sample_rate).into())
        .collect()
}

pub fn load_audio_file(
    path: &PathBuf,
    new_sample_rate: f32,
) -> Result<Vec<Arc<[f32]>>, AudioLoadError> {
    let extension = path
        .extension()
        .map(|ext| ext.to_str())
        .flatten()
        .ok_or_else(|| AudioLoadError::UnknownExtension)?;

    match extension {
        "wav" => Ok(load_wav(path, new_sample_rate)?),
        "flac" => Ok(load_flac(path, new_sample_rate)?),
        _ => Err(AudioLoadError::UnknownExtension),
    }
}

fn load_wav(path: &PathBuf, new_sample_rate: f32) -> io::Result<Vec<Arc<[f32]>>> {
    let mut reader = File::open(path)?;
    let (header, data) = wav::read(&mut reader)?;

    let vecs = extract_samples(data, header.channel_count);

    Ok(resample_vecs(
        vecs,
        header.sampling_rate as f32,
        new_sample_rate,
    ))
}

fn load_flac(
    path: &PathBuf,
    new_sample_rate: f32,
) -> Result<Vec<Arc<[f32]>>, AudioLoadError> {
    let path = path
        .to_str()
        .ok_or_else(|| AudioLoadError::UnknownExtension)?;

    match StreamReader::<File>::from_file(path) {
        Ok(mut stream) => {
            // Copy of `StreamInfo` to help convert to a different audio format.
            let info = stream.info();

            let bit_div = 1 << info.bits_per_sample;

            let vecs = build_arrays(stream.iter::<i32>(), info.channels as u16, |val| {
                (val as f32) / bit_div as f32
            });

            return Ok(resample_vecs(
                vecs,
                info.sample_rate as f32,
                new_sample_rate,
            ));
        }
        Err(ErrorKind::IO(io_err)) => Err(io::Error::from(io_err).into()),
        Err(_) => Err(AudioLoadError::FlacParseError),
    }
}