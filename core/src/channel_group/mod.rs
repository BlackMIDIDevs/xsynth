use std::sync::Arc;

use crate::{
    channel::{ChannelAudioEvent, ChannelEvent, VoiceChannel},
    helpers::sum_simd,
    AudioPipe, AudioStreamParams,
};

mod events;
pub use events::*;
use rayon::prelude::*;

const MAX_EVENT_CACHE_SIZE: u32 = 1024 * 1024;

pub struct ChannelGroup {
    thread_pool: rayon::ThreadPool,
    cached_event_count: u32,
    channel_events_cache: Box<[Vec<ChannelAudioEvent>]>,
    sample_cache_vecs: Box<[Vec<f32>]>,
    channels: Box<[VoiceChannel]>,
}

pub struct ChannelGroupConfig {
    pub channel_count: u32,
    pub audio_params: AudioStreamParams,
    pub use_threadpool: bool,
}

impl ChannelGroup {
    pub fn new(config: ChannelGroupConfig) -> Self {
        let mut channels = Vec::new();
        let mut channel_events_cache = Vec::new();
        let mut sample_cache_vecs = Vec::new();

        // Thread pool for individual channels to split between keys
        let pool = if config.use_threadpool {
            Some(Arc::new(rayon::ThreadPoolBuilder::new().build().unwrap()))
        } else {
            None
        };

        for _ in 0..config.channel_count {
            channels.push(VoiceChannel::new(
                config.audio_params.sample_rate,
                config.audio_params.channels,
                pool.clone(),
            ));
            channel_events_cache.push(Vec::new());
            sample_cache_vecs.push(Vec::new());
        }

        // Thread pool for splitting channels between threads
        let thread_pool = rayon::ThreadPoolBuilder::new().build().unwrap();

        Self {
            thread_pool,
            cached_event_count: 0,
            channel_events_cache: channel_events_cache.into_boxed_slice(),
            channels: channels.into_boxed_slice(),
            sample_cache_vecs: sample_cache_vecs.into_boxed_slice(),
        }
    }

    pub fn send_event(&mut self, event: SynthEvent) {
        match event {
            SynthEvent::Channel(channel, event) => {
                self.channel_events_cache[channel as usize].push(event);
                self.cached_event_count += 1;
                if self.cached_event_count > MAX_EVENT_CACHE_SIZE {
                    self.flush_events();
                }
            }
            SynthEvent::AllChannels(event) => {
                for channel in self.channel_events_cache.iter_mut() {
                    channel.push(event.clone());
                }
                self.cached_event_count += self.channel_events_cache.len() as u32;
                if self.cached_event_count > MAX_EVENT_CACHE_SIZE {
                    self.flush_events();
                }
            }
            SynthEvent::ChannelConfig(config) => {
                for channel in self.channels.iter_mut() {
                    channel.process_event(ChannelEvent::Config(config.clone()));
                }
            }
        }
    }

    fn flush_events(&mut self) {
        if self.cached_event_count == 0 {
            return;
        }

        let thread_pool = &mut self.thread_pool;
        let channels = &mut self.channels;
        let channel_events_cache = &mut self.channel_events_cache;

        thread_pool.install(move || {
            channels
                .par_iter_mut()
                .zip(channel_events_cache.par_iter_mut())
                .for_each(|(channel, events)| {
                    channel.push_events_iter(events.drain(..).map(ChannelEvent::Audio));
                });
        });

        self.cached_event_count = 0;
    }

    pub fn render_to(&mut self, buffer: &mut [f32]) {
        self.flush_events();

        let thread_pool = &mut self.thread_pool;
        let channels = &mut self.channels;
        let sample_cache_vecs = &mut self.sample_cache_vecs;

        thread_pool.install(move || {
            channels
                .par_iter_mut()
                .zip(sample_cache_vecs.par_iter_mut())
                .for_each(|(channel, samples)| {
                    samples.resize(buffer.len(), 0.0);
                    channel.read_samples(samples.as_mut_slice());
                });

            for vec in sample_cache_vecs.iter_mut() {
                sum_simd(vec, buffer);
                vec.clear();
            }
        });
    }
}
