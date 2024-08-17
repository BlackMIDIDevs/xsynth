use std::sync::Arc;

use crate::{
    channel::{ChannelAudioEvent, ChannelEvent, VoiceChannel},
    helpers::sum_simd,
    AudioPipe, AudioStreamParams,
};

mod config;
pub use config::*;
mod events;
pub use events::*;
use rayon::prelude::*;

const MAX_EVENT_CACHE_SIZE: u32 = 1024 * 1024;

/// Represents a MIDI synthesizer within XSynth.
///
/// Manages multiple VoiceChannel objects at once. For info about MIDI CC
/// support, please see the documentation of the `VoiceChannel` struct.
pub struct ChannelGroup {
    thread_pool: Option<rayon::ThreadPool>,
    cached_event_count: u32,
    channel_events_cache: Box<[Vec<ChannelAudioEvent>]>,
    sample_cache_vecs: Box<[Vec<f32>]>,
    channels: Box<[VoiceChannel]>,
    audio_params: AudioStreamParams,
}

impl ChannelGroup {
    /// Creates a new ChannelGroup with the given configuration.
    /// See the `ChannelGroupConfig` documentation for the available options.
    pub fn new(config: ChannelGroupConfig) -> Self {
        let mut channels = Vec::new();
        let mut channel_events_cache = Vec::new();
        let mut sample_cache_vecs = Vec::new();

        // Thread pool for individual channels to split between keys
        let channel_pool = match config.parallelism.channel {
            ThreadCount::None => None,
            ThreadCount::Auto => Some(Arc::new(rayon::ThreadPoolBuilder::new().build().unwrap())),
            ThreadCount::Manual(threads) => Some(Arc::new(
                rayon::ThreadPoolBuilder::new()
                    .num_threads(threads)
                    .build()
                    .unwrap(),
            )),
        };

        // Thread pool for splitting channels between threads
        let group_pool = match config.parallelism.key {
            ThreadCount::None => None,
            ThreadCount::Auto => Some(rayon::ThreadPoolBuilder::new().build().unwrap()),
            ThreadCount::Manual(threads) => Some(
                rayon::ThreadPoolBuilder::new()
                    .num_threads(threads)
                    .build()
                    .unwrap(),
            ),
        };

        for i in 0..config.channel_count {
            let mut init = config.channel_init_options;
            init.drums_only = config.drums_channels.clone().into_iter().any(|c| c == i);

            channels.push(VoiceChannel::new(
                init,
                config.audio_params,
                channel_pool.clone(),
            ));
            channel_events_cache.push(Vec::new());
            sample_cache_vecs.push(Vec::new());
        }

        Self {
            thread_pool: group_pool,
            cached_event_count: 0,
            channel_events_cache: channel_events_cache.into_boxed_slice(),
            channels: channels.into_boxed_slice(),
            sample_cache_vecs: sample_cache_vecs.into_boxed_slice(),
            audio_params: config.audio_params,
        }
    }

    /// Sends a SynthEvent to the ChannelGroup.
    /// See the `SynthEvent` documentation for more information.
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
                    channel.push(event);
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

        match self.thread_pool.as_ref() {
            Some(pool) => {
                let channels = &mut self.channels;
                let channel_events_cache = &mut self.channel_events_cache;

                pool.install(move || {
                    channels
                        .par_iter_mut()
                        .zip(channel_events_cache.par_iter_mut())
                        .for_each(|(channel, events)| {
                            channel.push_events_iter(events.drain(..).map(ChannelEvent::Audio));
                        });
                });
            }
            None => {
                for (channel, events) in self
                    .channels
                    .iter_mut()
                    .zip(self.channel_events_cache.iter_mut())
                {
                    channel.push_events_iter(events.drain(..).map(ChannelEvent::Audio));
                }
            }
        }

        self.cached_event_count = 0;
    }

    fn render_to(&mut self, buffer: &mut [f32]) {
        self.flush_events();
        buffer.fill(0.0);

        match self.thread_pool.as_ref() {
            Some(pool) => {
                let channels = &mut self.channels;
                let sample_cache_vecs = &mut self.sample_cache_vecs;
                pool.install(move || {
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
            None => {
                let len = buffer.len();

                for (channel, samples) in self
                    .channels
                    .iter_mut()
                    .zip(self.sample_cache_vecs.iter_mut())
                {
                    samples.resize(len, 0.0);
                    channel.read_samples(samples.as_mut_slice());
                }

                for vec in self.sample_cache_vecs.iter_mut() {
                    sum_simd(vec, buffer);
                    vec.clear();
                }
            }
        }
    }

    /// Returns the active voice count of the synthesizer.
    pub fn voice_count(&self) -> u64 {
        self.channels
            .iter()
            .map(|c| c.get_channel_stats().voice_count())
            .sum()
    }
}

impl AudioPipe for ChannelGroup {
    fn stream_params(&self) -> &AudioStreamParams {
        &self.audio_params
    }

    fn read_samples_unchecked(&mut self, to: &mut [f32]) {
        self.render_to(to);
    }
}
