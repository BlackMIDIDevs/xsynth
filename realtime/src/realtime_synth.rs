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
    Device, PauseStreamError, PlayStreamError, SizedSample, Stream, SupportedStreamConfig,
};
use crossbeam_channel::{bounded, unbounded};

use xsynth_core::{
    buffered_renderer::{BufferedRenderer, BufferedRendererStatsReader},
    channel::{ChannelConfigEvent, ChannelEvent, VoiceChannel},
    effects::VolumeLimiter,
    helpers::{prepapre_cache_vec, sum_simd},
    AudioPipe, AudioStreamParams, FunctionAudioPipe,
};

use crate::{
    util::ReadWriteAtomicU64, RealtimeEventSender, SynthEvent, ThreadCount, XSynthRealtimeConfig,
};

/// Holds the statistics for an instance of RealtimeSynth.
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

/// Reads the statistics of an instance of RealtimeSynth in a usable way.
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

    /// Returns the active voice count of all the MIDI channels.
    pub fn voice_count(&self) -> u64 {
        self.stats.voice_count.load(Ordering::Relaxed)
    }

    /// Returns the statistics of the buffered renderer used.
    ///
    /// See the BufferedRendererStatsReader documentation for more information.
    pub fn buffer(&self) -> &BufferedRendererStatsReader {
        &self.buffered_stats
    }
}

struct RealtimeSynthThreadSharedData {
    buffered_renderer: Arc<Mutex<BufferedRenderer>>,

    stream: Stream,

    event_senders: RealtimeEventSender,
}

/// A realtime MIDI synthesizer using an audio device for output.
pub struct RealtimeSynth {
    data: Option<RealtimeSynthThreadSharedData>,
    join_handles: Vec<thread::JoinHandle<()>>,

    stats: RealtimeSynthStats,

    stream_params: AudioStreamParams,
}

impl RealtimeSynth {
    /// Initializes a new realtime synthesizer using the default config and
    /// the default audio output.
    pub fn open_with_all_defaults() -> Self {
        let host = cpal::default_host();

        let device = host
            .default_output_device()
            .expect("failed to find output device");
        println!("Output device: {}", device.name().unwrap());

        let stream_config = device.default_output_config().unwrap();

        RealtimeSynth::open(Default::default(), &device, stream_config)
    }

    /// Initializes as new realtime synthesizer using a given config and
    /// the default audio output.
    ///
    /// See the `XSynthRealtimeConfig` documentation for the available options.
    pub fn open_with_default_output(config: XSynthRealtimeConfig) -> Self {
        let host = cpal::default_host();

        let device = host
            .default_output_device()
            .expect("failed to find output device");
        println!("Output device: {}", device.name().unwrap());

        let stream_config = device.default_output_config().unwrap();

        RealtimeSynth::open(config, &device, stream_config)
    }

    /// Initializes a new realtime synthesizer using a given config and a
    /// specified audio output device.
    ///
    /// See the `XSynthRealtimeConfig` documentation for the available options.
    /// See the `cpal` crate documentation for the `device` and `stream_config` parameters.
    pub fn open(
        config: XSynthRealtimeConfig,
        device: &Device,
        stream_config: SupportedStreamConfig,
    ) -> Self {
        let mut channel_stats = Vec::new();
        let mut senders = Vec::new();
        let mut command_senders = Vec::new();

        let sample_rate = stream_config.sample_rate().0;
        let stream_params = AudioStreamParams::new(sample_rate, stream_config.channels().into());

        let pool = match config.multithreading {
            ThreadCount::None => None,
            ThreadCount::Auto => Some(Arc::new(rayon::ThreadPoolBuilder::new().build().unwrap())),
            ThreadCount::Manual(threads) => Some(Arc::new(
                rayon::ThreadPoolBuilder::new()
                    .num_threads(threads)
                    .build()
                    .unwrap(),
            )),
        };

        let (output_sender, output_receiver) = bounded::<Vec<f32>>(config.channel_count as usize);

        let mut thread_handles = vec![];

        for _ in 0u32..(config.channel_count) {
            let mut channel =
                VoiceChannel::new(config.channel_init_options, stream_params, pool.clone());
            let stats = channel.get_channel_stats();
            channel_stats.push(stats);

            let (event_sender, event_receiver) = unbounded();
            senders.push(event_sender);

            let (command_sender, command_receiver) = bounded::<Vec<f32>>(1);

            command_senders.push(command_sender);

            let output_sender = output_sender.clone();
            let join_handle = thread::Builder::new()
                .name("xsynth_channel_handler".to_string())
                .spawn(move || loop {
                    channel.push_events_iter(event_receiver.try_iter());
                    let mut vec = match command_receiver.recv() {
                        Ok(vec) => vec,
                        Err(_) => break,
                    };
                    channel.push_events_iter(event_receiver.try_iter());
                    channel.read_samples(&mut vec);
                    output_sender.send(vec).unwrap();
                })
                .unwrap();

            thread_handles.push(join_handle);
        }

        if config.channel_count >= 16 {
            senders[9]
                .send(ChannelEvent::Config(ChannelConfigEvent::SetPercussionMode(
                    true,
                )))
                .unwrap();
        }

        let mut vec_cache: VecDeque<Vec<f32>> = VecDeque::new();
        for _ in 0..(config.channel_count) {
            vec_cache.push_front(Vec::new());
        }

        let stats = RealtimeSynthStats::new();

        let total_voice_count = stats.voice_count.clone();

        let channel_count = config.channel_count;
        let render = FunctionAudioPipe::new(stream_params, move |out| {
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
            stream_params,
            (sample_rate as f64 * config.render_window_ms / 1000.0) as usize,
        )));

        fn build_stream<T: SizedSample + ConvertSample>(
            device: &Device,
            stream_config: SupportedStreamConfig,
            buffered: Arc<Mutex<BufferedRenderer>>,
        ) -> Stream {
            let err_fn = |err| eprintln!("an error occurred on stream: {err}");
            let mut output_vec = Vec::new();

            let mut limiter = VolumeLimiter::new(stream_config.channels());

            device
                .build_output_stream(
                    &stream_config.into(),
                    move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                        output_vec.resize(data.len(), 0.0);
                        buffered.lock().unwrap().read(&mut output_vec);
                        for (i, s) in limiter.limit_iter(output_vec.drain(0..)).enumerate() {
                            data[i] = ConvertSample::from_f32(s);
                        }
                    },
                    err_fn,
                    None,
                )
                .unwrap()
        }

        let stream = match stream_config.sample_format() {
            cpal::SampleFormat::F32 => build_stream::<f32>(device, stream_config, buffered.clone()),
            cpal::SampleFormat::I16 => build_stream::<i16>(device, stream_config, buffered.clone()),
            cpal::SampleFormat::U16 => build_stream::<u16>(device, stream_config, buffered.clone()),
            _ => panic!("unsupported sample format"), // I hate when crates use #[non_exhaustive]
        };

        stream.play().unwrap();

        let max_nps = Arc::new(ReadWriteAtomicU64::new(10000));

        Self {
            data: Some(RealtimeSynthThreadSharedData {
                buffered_renderer: buffered,

                event_senders: RealtimeEventSender::new(senders, max_nps, config.ignore_range),
                stream,
            }),
            join_handles: thread_handles,

            stats,
            stream_params,
        }
    }

    /// Sends a SynthEvent to the realtime synthesizer.
    ///
    /// See the `SynthEvent` documentation for more information.
    pub fn send_event(&mut self, event: SynthEvent) {
        let data = self.data.as_mut().unwrap();
        data.event_senders.send_event(event);
    }

    /// Returns the event sender of the realtime synthesizer.
    ///
    /// See the `RealtimeEventSender` documentation for more information
    /// on how to use.
    pub fn get_senders(&self) -> RealtimeEventSender {
        let data = self.data.as_ref().unwrap();
        data.event_senders.clone()
    }

    /// Returns the statistics reader of the realtime synthesizer.
    ///
    /// See the `RealtimeSynthStatsReader` documentation for more information
    /// on how to use.
    pub fn get_stats(&self) -> RealtimeSynthStatsReader {
        let data = self.data.as_ref().unwrap();
        let buffered_stats = data.buffered_renderer.lock().unwrap().get_buffer_stats();

        RealtimeSynthStatsReader::new(self.stats.clone(), buffered_stats)
    }

    /// Returns the stream parameters of the audio output device.
    pub fn stream_params(&self) -> AudioStreamParams {
        self.stream_params
    }

    /// Pauses the playback of the audio output device.
    pub fn pause(&mut self) -> Result<(), PauseStreamError> {
        let data = self.data.as_mut().unwrap();
        data.stream.pause()
    }

    /// Resumes the playback of the audio output device.
    pub fn resume(&mut self) -> Result<(), PlayStreamError> {
        let data = self.data.as_mut().unwrap();
        data.stream.play()
    }
}

impl Drop for RealtimeSynth {
    fn drop(&mut self) {
        let data = self.data.take().unwrap();
        // data.stream.pause().unwrap();
        drop(data);
        for handle in self.join_handles.drain(..) {
            handle.join().unwrap();
        }
    }
}

trait ConvertSample: SizedSample {
    fn from_f32(s: f32) -> Self;
}

impl ConvertSample for f32 {
    fn from_f32(s: f32) -> Self {
        s
    }
}

impl ConvertSample for i16 {
    fn from_f32(s: f32) -> Self {
        (s * i16::MAX as f32) as i16
    }
}

impl ConvertSample for u16 {
    fn from_f32(s: f32) -> Self {
        ((s * u16::MAX as f32) as i32 + i16::MIN as i32) as u16
    }
}
