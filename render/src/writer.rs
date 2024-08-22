use crate::config::XSynthRenderConfig;

use std::{fs::File, io::BufWriter, path::PathBuf};

use hound::{WavSpec, WavWriter};

pub struct AudioFileWriter {
    writer: WavWriter<BufWriter<File>>,
}

impl AudioFileWriter {
    pub fn new(config: XSynthRenderConfig, path: PathBuf) -> Self {
        let spec = WavSpec {
            channels: config.group_options.audio_params.channels.count(),
            sample_rate: config.group_options.audio_params.sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let writer = WavWriter::create(path, spec).unwrap();

        Self { writer }
    }

    pub fn write_samples(&mut self, samples: &mut Vec<f32>) {
        for s in samples.drain(0..) {
            self.writer.write_sample(s).unwrap();
        }
    }

    pub fn finalize(self) {
        self.writer.finalize().unwrap();
    }
}
