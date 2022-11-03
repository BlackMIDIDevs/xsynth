use crossbeam_channel::{bounded, unbounded};

use core::{
    channel::{VoiceChannel, ChannelConfigEvent, ChannelAudioEvent, ControlEvent},
    effects::VolumeLimiter,
    helpers::{prepapre_cache_vec, sum_simd},
    AudioPipe, AudioStreamParams, BufferedRenderer, BufferedRendererStatsReader, FunctionAudioPipe,
    channel_group::{ChannelGroup, ChannelGroupConfig, SynthEvent},
    soundfont::SoundfontBase,
};

use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
    },
    thread::{self},
    path::PathBuf,
    time::{Duration, Instant},
};

use crate::{
    config::{XSynthRenderConfig, XSynthRenderAudioFormat}, RenderEventSender,
    writer::{AudioFileWriter, AudioWriterState},
};

use midi_toolkit::{
    events::{Event, MIDIEvent},
    io::MIDIFile,
    pipe,
    sequence::{
        event::{cancel_tempo_events, scale_event_time},
        unwrap_items, TimeCaster,
    },
};



pub struct XSynthRender {
    config: XSynthRenderConfig,
    channel_group: Arc<Mutex<ChannelGroup>>,
    audio_writer: Arc<Mutex<AudioFileWriter>>,
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
        let mut channel_group = Arc::new(Mutex::new(ChannelGroup::new(chgroup_config)));

        let mut audio_writer = Arc::new(Mutex::new(AudioFileWriter::new(config.audio_format, out_path)));

        Self {
            config: config.clone(),
            channel_group,
            audio_writer,
            audio_params,
        }
    }

    pub fn get_params(&self) -> AudioStreamParams {
        self.audio_params.clone()
    }

    pub fn send_event(&mut self, event: SynthEvent) {
        self.channel_group.lock().unwrap().send_event(event);
    }

    pub fn start_render(&mut self) {
        fn send_smpl(channel_group: Arc<Mutex<ChannelGroup>>, writer: Arc<Mutex<AudioFileWriter>>, config: XSynthRenderConfig) {
            thread::spawn(move || loop {
                let mut output_vec = vec![0.0; config.sample_rate as usize / 2];
                channel_group.lock().unwrap().render_to(&mut output_vec);

                let mut out = if config.use_limiter {
                    let mut out = Vec::new();
                    let mut limiter = VolumeLimiter::new(config.audio_channels);
                    for s in limiter.limit_iter(output_vec.drain(0..)) {
                        out.push(s);
                    }
                    out
                } else {
                    output_vec
                };
                writer.lock().unwrap().write_samples(&mut out);
            });
        }
        send_smpl(self.channel_group.clone(), self.audio_writer.clone(), self.config.clone());
    }

    pub fn render_batch(&mut self, event_time: f64) {
        let mut samples = (self.config.sample_rate as f64 * event_time) as usize;
        while samples % self.config.audio_channels as usize != 0 {
            samples += 1;
        }
        let mut output_vec = vec![0.0; samples];
        self.channel_group.lock().unwrap().render_to(&mut output_vec);

        let mut out = if self.config.use_limiter {
            let mut out = Vec::new();
            let mut limiter = VolumeLimiter::new(self.config.audio_channels);
            for s in limiter.limit_iter(output_vec.drain(0..)) {
                out.push(s);
            }
            out
        } else {
            output_vec
        };
        self.audio_writer.lock().unwrap().write_samples(&mut out);
    }
}
