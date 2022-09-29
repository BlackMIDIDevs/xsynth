use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread::{self},
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, PauseStreamError, PlayStreamError, Sample, Stream, SupportedStreamConfig,
};
use crossbeam_channel::{bounded, unbounded};
use to_vec::ToVec;

use core::{
    channel::VoiceChannel,
    effects::VolumeLimiter,
    helpers::{prepapre_cache_vec, sum_simd},
    AudioPipe, AudioStreamParams, BufferedRenderer, BufferedRendererStatsReader, FunctionAudioPipe,
};

use crate::{
    config::XSynthRealtimeConfig, util::ReadWriteAtomicU64, RealtimeEventSender, SynthEvent,
};

#[derive(Debug, Clone)]
struct RealtimeSynthStats {
    voice_count: Arc<AtomicU64>,
}

impl RealtimeSynthStats {
    pub fn new() -> RealtimeSynthStats {
        RealtimeSynthStats {
            voice_count: Arc::new(AtomicU64::new(0)),
        }
    }
}

pub struct RealtimeSynthStatsReader {
    buffered_stats: BufferedRendererStatsReader,
    stats: RealtimeSynthStats,
}

impl RealtimeSynthStatsReader {
    pub(self) fn new(
        stats: RealtimeSynthStats,
        buffered_stats: BufferedRendererStatsReader,
    ) -> RealtimeSynthStatsReader {
        RealtimeSynthStatsReader {
            stats,
            buffered_stats,
        }
    }

    pub fn voice_count(&self) -> u64 {
        self.stats.voice_count.load(Ordering::Relaxed)
    }

    pub fn buffer(&self) -> &BufferedRendererStatsReader {
        &self.buffered_stats
    }
}

pub struct RealtimeSynth {
    // Kept for ownership
    _channels: Vec<VoiceChannel>,
    buffered_renderer: Arc<Mutex<BufferedRenderer>>,

    stream: Stream,

    event_senders: RealtimeEventSender,

    stats: RealtimeSynthStats,

    stream_params: AudioStreamParams,
}

impl RealtimeSynth {
    pub fn open_with_all_defaults() -> Self {
        let host = cpal::default_host();

        let device = host
            .default_output_device()
            .expect("failed to find output device");
        println!("Output device: {}", device.name().unwrap());

        let stream_config = device.default_output_config().unwrap();

        RealtimeSynth::open(Default::default(), &device, stream_config)
    }

    pub fn open_with_default_output(config: XSynthRealtimeConfig) -> Self {
        let host = cpal::default_host();

        let device = host
            .default_output_device()
            .expect("failed to find output device");
        println!("Output device: {}", device.name().unwrap());

        let stream_config = device.default_output_config().unwrap();

        RealtimeSynth::open(config, &device, stream_config)
    }

    pub fn open(
        config: XSynthRealtimeConfig,
        device: &Device,
        stream_config: SupportedStreamConfig,
    ) -> Self {
        let mut channels = Vec::new();
        let mut senders = Vec::new();
        let mut command_senders = Vec::new();

        let sample_rate = stream_config.sample_rate().0;
        let audio_channels = stream_config.channels();

        let pool = if config.use_threadpool {
            Some(Arc::new(rayon::ThreadPoolBuilder::new().build().unwrap()))
        } else {
            None
        };

        let (output_sender, output_receiver) = bounded::<Vec<f32>>(config.channel_count as usize);

        for _ in 0u32..(config.channel_count) {
            let mut channel = VoiceChannel::new(sample_rate, audio_channels, pool.clone());
            channels.push(channel.clone());
            let (event_sender, event_receiver) = unbounded();
            senders.push(event_sender);

            let (command_sender, command_receiver) = bounded::<Vec<f32>>(1);

            command_senders.push(command_sender);

            let output_sender = output_sender.clone();
            thread::spawn(move || loop {
                channel.push_events_iter(event_receiver.try_iter());
                let mut vec = match command_receiver.recv() {
                    Ok(vec) => vec,
                    Err(_) => break,
                };
                channel.push_events_iter(event_receiver.try_iter());
                channel.read_samples(&mut vec);
                output_sender.send(vec).unwrap();
            });
        }

        let mut vec_cache: VecDeque<Vec<f32>> = VecDeque::new();
        for _ in 0..(config.channel_count) {
            vec_cache.push_front(Vec::new());
        }

        let stats = RealtimeSynthStats::new();

        let total_voice_count = stats.voice_count.clone();
        let channel_stats = channels.iter().map(|c| c.get_channel_stats()).to_vec();

        let channel_count = config.channel_count;
        let render = FunctionAudioPipe::new(sample_rate, audio_channels, move |out| {
            for sender in command_senders.iter() {
                let mut buf = vec_cache.pop_front().unwrap();
                prepapre_cache_vec(&mut buf, out.len(), 0.0);

                sender.send(buf).unwrap();
            }

            for _ in 0..channel_count {
                let buf = output_receiver.recv().unwrap();
                sum_simd(&buf, out);
                vec_cache.push_front(buf);
            }

            let total_voices = channel_stats.iter().map(|c| c.voice_count()).sum();
            total_voice_count.store(total_voices, Ordering::SeqCst);
        });

        let buffered = Arc::new(Mutex::new(BufferedRenderer::new(
            render,
            sample_rate,
            audio_channels,
            (sample_rate as f64 * config.render_window_ms / 1000.0) as usize,
        )));

        fn build_stream<T: Sample>(
            device: &Device,
            stream_config: SupportedStreamConfig,
            buffered: Arc<Mutex<BufferedRenderer>>,
        ) -> Stream {
            let err_fn = |err| eprintln!("an error occurred on stream: {}", err);
            let mut output_vec = Vec::new();

            let mut limiter = VolumeLimiter::new(stream_config.channels());

            device
                .build_output_stream(
                    &stream_config.into(),
                    move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                        output_vec.resize(data.len(), 0.0);
                        buffered.lock().unwrap().read(&mut output_vec);
                        for (i, s) in limiter.limit_iter(output_vec.drain(0..)).enumerate() {
                            data[i] = Sample::from(&s);
                        }
                    },
                    err_fn,
                )
                .unwrap()
        }

        let stream = match stream_config.sample_format() {
            cpal::SampleFormat::F32 => {
                build_stream::<f32>(device, stream_config, buffered.clone())
            }
            cpal::SampleFormat::I16 => {
                build_stream::<i16>(device, stream_config, buffered.clone())
            }
            cpal::SampleFormat::U16 => {
                build_stream::<u16>(device, stream_config, buffered.clone())
            }
        };

        stream.play().unwrap();

        let max_nps = Arc::new(ReadWriteAtomicU64::new(10000));

        Self {
            _channels: channels,
            buffered_renderer: buffered,

            event_senders: RealtimeEventSender::new(senders, max_nps),
            stream,
            stats,
            stream_params: AudioStreamParams::new(sample_rate, audio_channels),
        }
    }

    pub fn send_event(&mut self, event: SynthEvent) {
        self.event_senders.send_event(event);
    }

    pub fn get_senders(&self) -> RealtimeEventSender {
        self.event_senders.clone()
    }

    pub fn get_stats(&self) -> RealtimeSynthStatsReader {
        let buffered_stats = self.buffered_renderer.lock().unwrap().get_buffer_stats();

        RealtimeSynthStatsReader::new(self.stats.clone(), buffered_stats)
    }

    pub fn stream_params(&self) -> &AudioStreamParams {
        &self.stream_params
    }

    pub fn pause(&mut self) -> Result<(), PauseStreamError> {
        self.stream.pause()
    }

    pub fn resume(&mut self) -> Result<(), PlayStreamError> {
        self.stream.play()
    }
}
