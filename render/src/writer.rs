use crate::{
    config::XSynthRenderAudioFormat,
};

use std::{
    path::PathBuf,
    thread::{self},
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
    pub fn new(format: XSynthRenderAudioFormat, path: PathBuf) -> Self {
        match format {
            XSynthRenderAudioFormat::Wav => {
                let spec = WavSpec {
                    channels: 2,
                    sample_rate: 44100,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                };
                let mut writer = WavWriter::create(path, spec).unwrap();

                Self {
                    format,
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
                            writer.write_sample((s * 32768f32).round() as i16).unwrap();
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
