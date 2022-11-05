use core::{
    effects::VolumeLimiter,
    AudioStreamParams, AudioPipe,
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
    limiter: Option<VolumeLimiter>,
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

        let limiter = if config.use_limiter {
            Some(VolumeLimiter::new(config.audio_channels))
        } else {
            None
        };

        Self {
            config: config,
            channel_group,
            audio_writer,
            audio_params,
            limiter,
        }
    }

    pub fn get_params(&self) -> AudioStreamParams {
        self.audio_params.clone()
    }

    pub fn send_event(&mut self, event: SynthEvent) {
        self.channel_group.send_event(event);
    }

    pub fn render_batch(&mut self, event_time: f64) {
        let samples = ((self.config.sample_rate as f64 * event_time) as usize) * self.config.audio_channels as usize;
        let mut output_vec = Vec::new();
        output_vec.resize(samples, 0.0);
        self.channel_group.read_samples(&mut output_vec);

        if let Some(limiter) = &mut self.limiter {
            limiter.limit(&mut output_vec);
        }

        self.audio_writer.write_samples(&mut output_vec);
    }

    pub fn finalize(self) {
        self.audio_writer.finalize();
    }
}
