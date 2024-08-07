use xsynth_core::{
    channel_group::{ChannelGroup, SynthEvent},
    effects::VolumeLimiter,
    AudioPipe, AudioStreamParams,
};

use std::path::PathBuf;

use crate::{config::XSynthRenderConfig, writer::AudioFileWriter};

struct BatchRenderElements {
    output_vec: Vec<f32>,
    missed_samples: f64,
}

/// Represents an XSynth MIDI synthesizer that renders a MIDI to a file.
pub struct XSynthRender {
    config: XSynthRenderConfig,
    channel_group: ChannelGroup,
    audio_writer: AudioFileWriter,
    limiter: Option<VolumeLimiter>,
    render_elements: BatchRenderElements,
}

impl XSynthRender {
    /// Initializes a new XSynthRender object with the given configuration and
    /// audio output path.
    pub fn new(config: XSynthRenderConfig, out_path: PathBuf) -> Self {
        let channel_group = ChannelGroup::new(config.group_options.clone());

        let audio_writer = AudioFileWriter::new(config.clone(), out_path);

        let limiter = if config.use_limiter {
            Some(VolumeLimiter::new(
                config.group_options.audio_params.channels.count(),
            ))
        } else {
            None
        };

        Self {
            config,
            channel_group,
            audio_writer,
            limiter,
            render_elements: BatchRenderElements {
                output_vec: vec![0.0],
                missed_samples: 0.0,
            },
        }
    }

    /// Returns the parameters of the output audio.
    pub fn get_params(&self) -> AudioStreamParams {
        self.config.group_options.audio_params
    }

    /// Sends a SynthEvent to the XSynthRender object.
    /// Please see the SynthEvent documentation for more information.
    pub fn send_event(&mut self, event: SynthEvent) {
        self.channel_group.send_event(event);
    }

    /// Renders audio samples of the specified time to the audio output file.
    ///
    /// The time should be the delta time of the last sent events.
    pub fn render_batch(&mut self, event_time: f64) {
        if event_time > 10.0 {
            // If the time is too large, split it up
            let mut remaining_time = event_time;
            loop {
                if remaining_time > 10.0 {
                    self.render_batch(10.0);
                    remaining_time -= 10.0;
                } else {
                    self.render_batch(remaining_time);
                    break;
                }
            }
        } else {
            let samples = self.config.group_options.audio_params.sample_rate as f64 * event_time
                + self.render_elements.missed_samples;
            self.render_elements.missed_samples = samples % 1.0;
            let samples =
                samples as usize * self.config.group_options.audio_params.channels.count() as usize;

            self.render_elements.output_vec.resize(samples, 0.0);
            self.channel_group
                .read_samples(&mut self.render_elements.output_vec);

            if let Some(limiter) = &mut self.limiter {
                limiter.limit(&mut self.render_elements.output_vec);
            }

            self.audio_writer
                .write_samples(&mut self.render_elements.output_vec);
        }
    }

    /// Finishes the render and finalizes the audio file.
    pub fn finalize(mut self) {
        loop {
            self.render_elements.output_vec.resize(
                self.config.group_options.audio_params.sample_rate as usize,
                0.0,
            );
            self.channel_group
                .read_samples(&mut self.render_elements.output_vec);
            let mut is_empty = true;
            for s in &self.render_elements.output_vec {
                if *s > 0.0001 || *s < -0.0001 {
                    is_empty = false;
                    break;
                }
            }
            if is_empty {
                break;
            }
            self.audio_writer
                .write_samples(&mut self.render_elements.output_vec);
        }
        self.audio_writer.finalize();
    }

    /// Returns the active voice count of the MIDI synthesizer.
    pub fn voice_count(&self) -> u64 {
        self.channel_group.voice_count()
    }
}
