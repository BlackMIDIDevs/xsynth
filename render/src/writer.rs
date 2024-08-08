use crate::config::{XSynthRenderAudioFormat, XSynthRenderConfig};

use std::{fs::File, io::BufWriter, path::PathBuf};

use hound::{WavSpec, WavWriter};

pub struct AudioFileWriter {
    config: XSynthRenderConfig,
    wav_writer: Option<WavWriter<BufWriter<File>>>,
}

impl AudioFileWriter {
    pub fn new(config: XSynthRenderConfig, path: PathBuf) -> Self {
        match config.audio_format {
            XSynthRenderAudioFormat::Wav => {
                let spec = WavSpec {
                    channels: config.group_options.audio_params.channels.count(),
                    sample_rate: config.group_options.audio_params.sample_rate,
                    bits_per_sample: 32,
                    sample_format: hound::SampleFormat::Float,
                };
                let writer = WavWriter::create(path, spec).unwrap();

                Self {
                    config,
                    wav_writer: Some(writer),
                }
            }
        }
    }

    pub fn write_samples(&mut self, samples: &mut Vec<f32>) {
        match self.config.audio_format {
            XSynthRenderAudioFormat::Wav => {
                for s in samples.drain(0..) {
                    if let Some(writer) = &mut self.wav_writer {
                        writer.write_sample(s).unwrap();
                    }
                }
            }
        }
    }

    pub fn finalize(mut self) {
        match self.config.audio_format {
            XSynthRenderAudioFormat::Wav => {
                if let Some(writer) = self.wav_writer {
                    writer.finalize().unwrap();
                    self.wav_writer = None;
                }
            }
        }
    }
}
