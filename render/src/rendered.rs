use crossbeam_channel::{bounded, unbounded};

use core::{
    channel::{VoiceChannel, ChannelConfigEvent},
    effects::VolumeLimiter,
    helpers::{prepapre_cache_vec, sum_simd},
    AudioPipe, AudioStreamParams, BufferedRenderer, BufferedRendererStatsReader, FunctionAudioPipe,
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
    config::{XSynthRenderConfig, XSynthRenderAudioFormat}, RenderEventSender, SynthEvent,
    writer::{AudioFileWriter, AudioWriterState},
};



pub struct XSynthRender {
    buffered: Arc<Mutex<BufferedRenderer>>,
    config: XSynthRenderConfig,
    event_senders: RenderEventSender,
    audio_writer: Arc<Mutex<AudioFileWriter>>,
    stream_params: AudioStreamParams,
}

impl XSynthRender {
    pub fn open_renderer(config: XSynthRenderConfig, path: PathBuf) -> Self {
        let mut channels = Vec::new();
        let mut senders = Vec::new();
        let mut command_senders = Vec::new();

        let sample_rate = config.sample_rate;
        let audio_channels = config.audio_channels;

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
        });

        let buffered = Arc::new(Mutex::new(BufferedRenderer::new(
            render,
            sample_rate,
            audio_channels,
            (sample_rate / 1000) as usize,
        )));

        let mut audio_writer = Arc::new(Mutex::new(AudioFileWriter::new(config.audio_format, path)));

        fn send_smpl(buffered: Arc<Mutex<BufferedRenderer>>, writer: Arc<Mutex<AudioFileWriter>>, config: XSynthRenderConfig) {
            thread::spawn(move || {
                loop {
                    let mut output_vec = vec![0.0; config.sample_rate as usize];
                    buffered.lock().unwrap().read(&mut output_vec);

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
                    thread::sleep_ms(1);
                }
            });
        }

        send_smpl(buffered.clone(), audio_writer.clone(), config.clone());

        Self {
            buffered: buffered.clone(),
            config: config.clone(),
            event_senders: RenderEventSender::new(senders),
            audio_writer: audio_writer.clone(),
            stream_params: AudioStreamParams::new(sample_rate, audio_channels),
        }
    }

    pub fn send_event(&mut self, event: SynthEvent) {
        self.event_senders.send_event(event);
    }

    pub fn send_config(&mut self, event: ChannelConfigEvent) {
        self.event_senders.send_config(event);
    }

    pub fn get_senders(&self) -> RenderEventSender {
        self.event_senders.clone()
    }

    /*pub fn finalize(mut self) {
        self.audio_writer.lock().unwrap().finalize();
    }*/

    pub fn stream_params(&self) -> &AudioStreamParams {
        &self.stream_params
    }
}
