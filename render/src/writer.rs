use crate::config::XSynthRenderConfig;

use std::{path::PathBuf, thread};

use crossbeam_channel::Sender;
use hound::{WavSpec, WavWriter};

pub struct AudioFileWriter {
    sender: Sender<Vec<f32>>,
}

impl AudioFileWriter {
    pub fn new(config: XSynthRenderConfig, path: PathBuf) -> Self {
        let spec = WavSpec {
            channels: config.group_options.audio_params.channels.count(),
            sample_rate: config.group_options.audio_params.sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = WavWriter::create(path, spec).unwrap();

        let (snd, rcv) = crossbeam_channel::unbounded::<Vec<f32>>();

        thread::spawn(move || {
            for batch in rcv {
                for s in batch {
                    writer.write_sample(s).unwrap();
                }
            }
            writer.finalize().unwrap();
        });

        Self { sender: snd }
    }

    pub fn write_samples(&mut self, samples: &mut Vec<f32>) {
        self.sender.send(std::mem::take(samples)).unwrap();
    }
}
