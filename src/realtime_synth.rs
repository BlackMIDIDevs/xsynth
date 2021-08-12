use std::{collections::VecDeque, sync::Arc, thread, time::Instant};

use cpal::{
    traits::{DeviceTrait, StreamTrait},
    Device, Sample, Stream, StreamConfig, SupportedStreamConfig,
};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};

use crate::{
    core::{event::ChannelEvent, AudioPipe, BufferedRenderer, FunctionAudioPipe, VoiceChannel},
    helpers::sum_simd,
    SynthEvent,
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
        self.senders[event.channel as usize].send(event.event);
    }
}

pub struct RealtimeSynth {
    channels: Vec<VoiceChannel>,
    stream: Stream,

    event_senders: RealtimeEventSender,
}

impl RealtimeSynth {
    pub fn new(channel_count: u32, device: &Device, config: SupportedStreamConfig) -> Self {
        let mut channels = Vec::new();
        let mut senders = Vec::new();
        let mut command_senders = Vec::new();

        let sample_rate = config.sample_rate().0;
        let audio_channels = config.channels();

        let pool = Arc::new(rayon::ThreadPoolBuilder::new().build().unwrap());

        let (output_sender, output_receiver) = bounded::<Vec<f32>>(channel_count as usize);

        for _ in 0u32..channel_count {
            let mut channel = VoiceChannel::new(sample_rate, audio_channels, Some(pool.clone()));
            channels.push(channel.clone());
            let (event_sender, event_receiver) = unbounded();
            senders.push(event_sender);

            let (command_sender, command_receiver) = bounded::<Vec<f32>>(1);

            command_senders.push(command_sender);

            let output_sender = output_sender.clone();
            thread::spawn(move || loop {
                channel.push_events_iter(event_receiver.try_iter());
                let mut vec = command_receiver.recv().unwrap();
                channel.push_events_iter(event_receiver.try_iter());
                channel.read_samples(&mut vec);
                output_sender.send(vec).unwrap();
            });
        }

        let mut vec_cache: VecDeque<Vec<f32>> = VecDeque::new();
        for _ in 0..channel_count {
            vec_cache.push_front(Vec::new());
        }
        let render = FunctionAudioPipe::new(sample_rate, audio_channels, move |out| {
            // let now = Instant::now();
            for i in 0..channel_count as usize {
                let mut buf = vec_cache.pop_front().unwrap();
                buf.clear();
                buf.reserve(out.len());
                for _ in 0..out.len() {
                    buf.push(0.0);
                }

                let channel = &command_senders[i];
                channel.send(buf).unwrap();
            }
            // println!("Samples: {:?}", now.elapsed());

            for _ in 0..channel_count {
                let buf = output_receiver.recv().unwrap();
                sum_simd(&buf, out);
                vec_cache.push_front(buf);
            }
        });

        let buffered = BufferedRenderer::new(render, sample_rate, audio_channels, 48);

        fn build_stream<T: Sample>(
            device: &Device,
            config: SupportedStreamConfig,
            mut buffered: BufferedRenderer,
        ) -> Stream {
            let err_fn = |err| eprintln!("an error occurred on stream: {}", err);
            let mut output_vec = Vec::new();

            let stream = device
                .build_output_stream(
                    &config.into(),
                    move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                        output_vec.reserve(data.len());
                        for _ in 0..data.len() {
                            output_vec.push(0.0);
                        }
                        buffered.read(&mut output_vec);
                        let mut i = 0;
                        for s in output_vec.drain(0..) {
                            data[i] = Sample::from(&s);
                            i += 1;
                        }
                    },
                    err_fn,
                )
                .unwrap();

            stream
        }

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => build_stream::<f32>(&device, config, buffered),
            cpal::SampleFormat::I16 => build_stream::<i16>(&device, config, buffered),
            cpal::SampleFormat::U16 => build_stream::<u16>(&device, config, buffered),
        };

        stream.play().unwrap();

        Self {
            channels,
            event_senders: RealtimeEventSender::new(senders),
            stream,
        }
    }

    pub fn send_event(&self, event: SynthEvent) {
        self.event_senders.send(event);
    }
}
