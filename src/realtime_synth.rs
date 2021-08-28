use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
};

use cpal::{
    traits::{DeviceTrait, StreamTrait},
    Device, Sample, Stream, SupportedStreamConfig,
};
use crossbeam_channel::{bounded, unbounded, Sender};
use to_vec::ToVec;

use crate::{
    core::{
        effects::VolumeLimiter, event::ChannelEvent, AudioPipe, BufferedRenderer,
        FunctionAudioPipe, VoiceChannel,
    },
    helpers::{prepapre_cache_vec, sum_simd},
    AudioStreamParams, SynthEvent,
};

#[derive(Clone)]
struct RealtimeEventSender {
    senders: Vec<Sender<ChannelEvent>>,
}

impl RealtimeEventSender {
    pub fn new(senders: Vec<Sender<ChannelEvent>>) -> RealtimeEventSender {
        RealtimeEventSender { senders: senders }
    }

    pub fn send(&self, event: SynthEvent) {
        match event {
            SynthEvent::Channel(channel, event) => {
                self.senders[channel as usize].send(event).ok();
            }
            SynthEvent::SetSoundfonts(soundfonts) => {
                for sender in self.senders.iter() {
                    sender
                        .send(ChannelEvent::SetSoundfonts(soundfonts.clone()))
                        .ok();
                }
            }
        }
    }
}

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
    stats: RealtimeSynthStats,
}

impl RealtimeSynthStatsReader {
    pub(self) fn new(stats: RealtimeSynthStats) -> RealtimeSynthStatsReader {
        RealtimeSynthStatsReader { stats }
    }

    pub fn voice_count(&self) -> u64 {
        self.stats.voice_count.load(Ordering::Relaxed)
    }
}

pub struct RealtimeSynth {
    channels: Vec<VoiceChannel>,
    stream: Stream,

    event_senders: RealtimeEventSender,

    stats: RealtimeSynthStats,

    buffered_renderer: Arc<Mutex<BufferedRenderer>>,

    stream_params: AudioStreamParams,
}

impl RealtimeSynth {
    pub fn new(channel_count: u32, device: &Device, config: SupportedStreamConfig) -> Self {
        let mut channels = Vec::new();
        let mut senders = Vec::new();
        let mut command_senders = Vec::new();

        let sample_rate = config.sample_rate().0;
        let audio_channels = config.channels();

        // let pool = Arc::new(rayon::ThreadPoolBuilder::new().build().unwrap());

        let (output_sender, output_receiver) = bounded::<Vec<f32>>(channel_count as usize);

        for _ in 0u32..channel_count {
            let mut channel = VoiceChannel::new(sample_rate, audio_channels, None);
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
        for _ in 0..channel_count {
            vec_cache.push_front(Vec::new());
        }

        let stats = RealtimeSynthStats::new();

        let total_voice_count = stats.voice_count.clone();
        let channel_stats = channels.iter().map(|c| c.get_channel_stats()).to_vec();

        let render = FunctionAudioPipe::new(sample_rate, audio_channels, move |out| {
            for i in 0..channel_count as usize {
                let mut buf = vec_cache.pop_front().unwrap();
                prepapre_cache_vec(&mut buf, out.len(), 0.0);

                let channel = &command_senders[i];
                channel.send(buf).unwrap();
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
            48,
        )));

        fn build_stream<T: Sample>(
            device: &Device,
            config: SupportedStreamConfig,
            buffered: Arc<Mutex<BufferedRenderer>>,
            render_callback: Box<dyn Fn() + Send + 'static>,
        ) -> Stream {
            let err_fn = |err| eprintln!("an error occurred on stream: {}", err);
            let mut output_vec = Vec::new();

            let mut limiter = VolumeLimiter::new(config.channels());

            let stream = device
                .build_output_stream(
                    &config.into(),
                    move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                        output_vec.reserve(data.len());
                        for _ in 0..data.len() {
                            output_vec.push(0.0);
                        }
                        buffered.lock().unwrap().read(&mut output_vec);
                        let mut i = 0;
                        for s in limiter.limit_iter(output_vec.drain(0..)) {
                            data[i] = Sample::from(&s);
                            i += 1;
                        }

                        render_callback();
                    },
                    err_fn,
                )
                .unwrap();

            stream
        }

        let total_voice_count = stats.voice_count.clone();
        let render_callback = Box::new(move || {
            println!("Voice Count: {}", total_voice_count.load(Ordering::SeqCst));
        });

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                build_stream::<f32>(&device, config, buffered.clone(), render_callback)
            }
            cpal::SampleFormat::I16 => {
                build_stream::<i16>(&device, config, buffered.clone(), render_callback)
            }
            cpal::SampleFormat::U16 => {
                build_stream::<u16>(&device, config, buffered.clone(), render_callback)
            }
        };

        stream.play().unwrap();

        Self {
            channels,
            event_senders: RealtimeEventSender::new(senders),
            stream,
            buffered_renderer: buffered,
            stats,
            stream_params: AudioStreamParams::new(sample_rate, audio_channels),
        }
    }

    pub fn send_event(&self, event: SynthEvent) {
        self.event_senders.send(event);
    }

    pub fn get_stats(&self) -> RealtimeSynthStatsReader {
        RealtimeSynthStatsReader::new(self.stats.clone())
    }

    pub fn stream_params(&self) -> &AudioStreamParams {
        &self.stream_params
    }
}
