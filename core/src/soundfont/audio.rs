use std::{fs::File, io, path::PathBuf, sync::Arc};

use symphonia::core::audio::GenericAudioBufferRef;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::{AudioStreamParams, ChannelCount};
use thiserror::Error;
use xsynth_soundfonts::resample::resample_vecs;

/// Errors that can be generated when loading an audio file.
#[derive(Debug, Error)]
pub enum AudioLoadError {
    #[error("IO Error")]
    IOError(#[from] io::Error),

    #[error("Audio decoding failed for {0}")]
    AudioDecodingFailed(PathBuf, Error),

    #[error("Audio file {0} has an invalid channel count")]
    InvalidChannelCount(PathBuf),

    #[error("Audio file {0} has no tracks")]
    NoTracks(PathBuf),
}

type ProcessedSample = (Arc<[Arc<[f32]>]>, u32);

pub(super) fn load_audio_file(
    path: &PathBuf,
    stream_params: AudioStreamParams,
) -> Result<ProcessedSample, AudioLoadError> {
    let new_sample_rate = stream_params.sample_rate as f32;

    let extension = path.extension().and_then(|ext| ext.to_str());

    let file = Box::new(File::open(path)?);

    // Create the media source stream using the boxed media source from above.
    let mss = MediaSourceStream::new(file, Default::default());

    // Create a hint to help the format registry guess what format reader is appropriate.
    let mut hint = Hint::new();
    if let Some(extension) = extension {
        hint.with_extension(extension);
    }

    // Use the default options when reading and decoding.
    let format_opts: FormatOptions = Default::default();
    let metadata_opts: MetadataOptions = Default::default();
    let decoder_opts: AudioDecoderOptions = Default::default();

    // Probe the media source stream for a format.
    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, format_opts, metadata_opts)
        .map_err(|x| AudioLoadError::AudioDecodingFailed(path.clone(), x))?;

    // Get the default audio track.
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| AudioLoadError::NoTracks(path.clone()))?;

    let codec_params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| AudioLoadError::NoTracks(path.clone()))?;

    let sample_rate = codec_params.sample_rate.unwrap_or(44100);
    let channel_count = codec_params
        .channels
        .as_ref()
        .map(|c| c.count())
        .unwrap_or(1);

    // Create a decoder for the track.
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(codec_params, &decoder_opts)
        .map_err(|x| AudioLoadError::AudioDecodingFailed(path.clone(), x))?;

    // Store the track identifier, we'll use it to filter packets.
    let track_id = track.id;

    // Builder for the parsed audio buffers
    let mut builder = BuilderVecs::new(channel_count);

    loop {
        // Get the next packet from the format reader.
        let packet = match format.next_packet() {
            Ok(None) => break,
            Err(symphonia::core::errors::Error::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                // Audio source ended. Currently the lib has no cleaner way of detecting this.
                break;
            }
            Err(error) => return Err(AudioLoadError::AudioDecodingFailed(path.clone(), error)),
            Ok(Some(packet)) => packet,
        };

        // If the packet does not belong to the selected track, skip it.
        if packet.track_id != track_id {
            continue;
        }

        // Decode the packet into audio samples, ignoring any decode errors.
        match decoder.decode(&packet) {
            Ok(audio_buf) => {
                builder.push(audio_buf);
            }

            Err(Error::DecodeError(_)) => (),
            Err(e) => return Err(AudioLoadError::AudioDecodingFailed(path.clone(), e)),
        }
    }

    let built = builder.finish(sample_rate as f32, new_sample_rate, stream_params.channels);

    Ok((built, sample_rate))
}

struct BuilderVecs {
    vecs: Vec<Vec<f32>>,
}

impl BuilderVecs {
    fn new(channels: usize) -> Self {
        let mut vecs = Vec::new();
        for _ in 0..channels {
            vecs.push(Vec::new());
        }

        Self { vecs }
    }

    fn push(&mut self, buffer: GenericAudioBufferRef) {
        let mut planes: Vec<Vec<f32>> = Vec::new();
        buffer.copy_to_vecs_planar(&mut planes);

        for (vec, plane) in self.vecs.iter_mut().zip(planes) {
            vec.extend(plane);
        }
    }

    fn finish(
        self,
        sample_rate: f32,
        new_sample_rate: f32,
        channels: ChannelCount,
    ) -> Arc<[Arc<[f32]>]> {
        let mut vecs = self.vecs;

        if channels == ChannelCount::Mono && vecs.len() >= 2 {
            let right = vecs.pop().unwrap_or_default();
            let left = vecs.pop().unwrap_or_default();

            let combined: Vec<f32> = left
                .iter()
                .zip(right.iter())
                .map(|(&l, &r)| (l + r) * 0.5)
                .collect();
            vecs.push(combined);
        }

        for chan in vecs.iter_mut() {
            chan.shrink_to_fit();
        }

        resample_vecs(vecs, sample_rate, new_sample_rate)
    }
}
