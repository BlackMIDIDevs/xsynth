use core::{
    effects::VolumeLimiter,
    AudioStreamParams,
    channel_group::{ChannelGroup, ChannelGroupConfig, SynthEvent},
};

use std::{
    path::PathBuf,
};

use crate::{
    config::XSynthRenderConfig,
    writer::AudioFileWriter,
};



pub struct XSynthRender {
    config: XSynthRenderConfig,
    channel_group: ChannelGroup,
    audio_writer: AudioFileWriter,
    audio_params: AudioStreamParams,
}

impl XSynthRender {
    pub fn new(config: XSynthRenderConfig, out_path: PathBuf) -> Self {
        let audio_params = AudioStreamParams::new(config.sample_rate, config.audio_channels);
        let chgroup_config = ChannelGroupConfig {
            channel_count: config.channel_count,
            audio_params: audio_params.clone(),
            use_threadpool: config.use_threadpool,
        };
        let channel_group = ChannelGroup::new(chgroup_config);

        let audio_writer = AudioFileWriter::new(config.clone(), out_path);

        Self {
            config: config,
            channel_group,
            audio_writer,
            audio_params,
        }
    }

    pub fn get_params(&self) -> AudioStreamParams {
        self.audio_params.clone()
    }

    pub fn send_event(&mut self, event: SynthEvent) {
        self.channel_group.send_event(event);
    }

    pub fn render_batch(&mut self, event_time: f64) {
        let samples = (self.config.sample_rate as f64 * event_time) as u16 * self.config.audio_channels;
        let mut output_vec = Vec::new();
        output_vec.resize(samples as usize, 0.0);
        self.channel_group.render_to(&mut output_vec);

        if self.config.use_limiter {
            let limiter = VolumeLimiter::new(self.config.audio_channels);
            limiter.limit(&mut output_vec);
        }

        self.audio_writer.write_samples(&mut output_vec);
    }

    pub fn finalize(self) {
        self.audio_writer.finalize();
    }
}
