use crate::{
    config::{XSynthRenderConfig, XSynthRenderAudioFormat},
};

use std::{
    path::PathBuf,
    io::BufWriter,
    fs::File,
};

use hound::{WavWriter, WavSpec};


#[derive(PartialEq, Clone, Copy)]
pub enum AudioWriterState {
    Idle,
    Writing,
    Finished,
}

pub struct AudioFileWriter {
    format: XSynthRenderAudioFormat,
    state: AudioWriterState,
    wav_writer: Option<WavWriter<BufWriter<File>>>,
}

impl AudioFileWriter {
    pub fn new(config: XSynthRenderConfig, path: PathBuf) -> Self {
        match config.audio_format {
            XSynthRenderAudioFormat::Wav => {
                let spec = WavSpec {
                    channels: config.audio_channels,
                    sample_rate: config.sample_rate,
                    bits_per_sample: 32,
                    sample_format: hound::SampleFormat::Float,
                };
                let writer = WavWriter::create(path, spec).unwrap();

                Self {
                    format: config.audio_format,
                    state: AudioWriterState::Idle,
                    wav_writer: Some(writer),
                }
            }
            _ => {
                Self {
                    format: XSynthRenderAudioFormat::Wav,
                    state: AudioWriterState::Finished,
                    wav_writer: None,
                }
            },
        }
    }

    pub fn write_samples(&mut self, samples: &mut Vec<f32>) {
        match self.format {
            XSynthRenderAudioFormat::Wav => {
                for s in samples.drain(..) {
                    // Ignore blank at beginning
                    if self.state == AudioWriterState::Idle {
                        if s != 0.0 {
                            self.state = AudioWriterState::Writing;
                        }
                    } else {
                        if let Some(writer) = &mut self.wav_writer {
                            writer.write_sample(s).unwrap();
                        }
                    };
                }
            },
            _ => {},
        }
    }

    pub fn finalize(mut self) {
        match self.format {
            XSynthRenderAudioFormat::Wav => {
                if let Some(writer) = self.wav_writer {
                    writer.finalize().unwrap();
                    self.wav_writer = None;
                    self.state = AudioWriterState::Finished;
                }
            }
            _ => {},
        }
    }

    pub fn get_state(&self) -> AudioWriterState {
        self.state
    }
}
