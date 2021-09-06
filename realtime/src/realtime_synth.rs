use std::{
    cell::UnsafeCell,
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, RwLock,
    },
    thread::{self},
    time::{Duration, Instant},
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, PauseStreamError, PlayStreamError, Sample, Stream, SupportedStreamConfig,
};
use crossbeam_channel::{bounded, unbounded, Sender};
use to_vec::ToVec;

use core::{
    channel::{ChannelEvent, VoiceChannel},
    effects::VolumeLimiter,
    helpers::{prepapre_cache_vec, sum_simd},
    AudioPipe, AudioStreamParams, BufferedRenderer, BufferedRendererStatsReader, FunctionAudioPipe,
};

use crate::SynthEvent;

struct ReadWriteAtomicU64(UnsafeCell<u64>);

impl ReadWriteAtomicU64 {
    fn new(value: u64) -> Self {
        ReadWriteAtomicU64(UnsafeCell::new(value))
    }

    fn read(&self) -> u64 {
        unsafe { *self.0.get() }
    }

    fn write(&self, value: u64) {
        unsafe { *self.0.get() = value }
    }
}

unsafe impl Send for ReadWriteAtomicU64 {}
unsafe impl Sync for ReadWriteAtomicU64 {}

static NPS_WINDOW_MILLISECONDS: u64 = 20;

struct NpsWindow {
    time: u64,
    notes: u64,
}

/// A struct for tracking the estimated NPS, as fast as possible with the focus on speed
/// rather than precision. Used for NPS limiting on extremely spammy midis.
struct RoughNpsTracker {
    rough_time: Arc<ReadWriteAtomicU64>,
    last_time: u64,
    windows: VecDeque<NpsWindow>,
    total_window_sum: u64,
    current_window_sum: u64,
    stop: Arc<RwLock<bool>>,
}

impl RoughNpsTracker {
    pub fn new() -> RoughNpsTracker {
        let rough_time = Arc::new(ReadWriteAtomicU64::new(0));
        let stop = Arc::new(RwLock::new(false));

        {
            let rough_time = rough_time.clone();
            let stop = stop.clone();
            thread::spawn(move || {
                let mut last_time = 0;
                let mut now = Instant::now();
                while *stop.read().unwrap() == false {
                    thread::sleep(Duration::from_millis(NPS_WINDOW_MILLISECONDS));
                    let diff = now.elapsed();
                    last_time += diff.as_millis() as u64;
                    rough_time.write(last_time);
                    now = Instant::now();
                }
            });
        }

        RoughNpsTracker {
            rough_time,
            last_time: 0,
            windows: VecDeque::new(),
            total_window_sum: 0,
            current_window_sum: 0,
            stop,
        }
    }

    pub fn calculate_nps(&mut self) -> u64 {
        self.check_time();

        loop {
            let cutoff = self.last_time - 1000;
            if let Some(window) = self.windows.front() {
                if window.time < cutoff {
                    self.total_window_sum -= window.notes;
                    self.windows.pop_front();
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        let short_nps = self.current_window_sum * (1000 / NPS_WINDOW_MILLISECONDS) * 4 / 3;
        let long_nps = self.total_window_sum;

        short_nps.max(long_nps)
    }

    fn check_time(&mut self) {
        let time = self.rough_time.read();
        if time > self.last_time {
            self.windows.push_back(NpsWindow {
                time: self.last_time,
                notes: self.current_window_sum,
            });
            self.current_window_sum = 0;
            self.last_time = time;
        }
    }

    pub fn add_note(&mut self) {
        self.current_window_sum += 1;
        self.total_window_sum += 1;
    }
}

impl Drop for RoughNpsTracker {
    fn drop(&mut self) {
        *self.stop.write().unwrap() = true;
    }
}

fn should_send_for_vel_and_nps(vel: u8, nps: u64, max: u64) -> bool {
    vel as u64 * 100 + max > nps
}

struct EventSender {
    sender: Sender<ChannelEvent>,
    nps: RoughNpsTracker,
    max_nps: Arc<ReadWriteAtomicU64>,
    skipped_notes: [u64; 128],
}

impl EventSender {
    pub fn new(max_nps: Arc<ReadWriteAtomicU64>, sender: Sender<ChannelEvent>) -> Self {
        EventSender {
            sender,
            nps: RoughNpsTracker::new(),
            max_nps,
            skipped_notes: [0; 128],
        }
    }

    pub fn send(&mut self, event: ChannelEvent) {
        match &event {
            ChannelEvent::NoteOn { vel, key } => {
                let nps = self.nps.calculate_nps();
                if should_send_for_vel_and_nps(*vel, nps, self.max_nps.read()) {
                    self.sender.send(event).ok();
                    self.nps.add_note();
                } else {
                    self.skipped_notes[*key as usize] += 1;
                }
            }
            ChannelEvent::NoteOff { key } => {
                if self.skipped_notes[*key as usize] > 0 {
                    self.skipped_notes[*key as usize] -= 1;
                } else {
                    self.sender.send(event).ok();
                }
            }
            _ => {
                self.sender.send(event).ok();
            }
        }
    }
}

impl Clone for EventSender {
    fn clone(&self) -> Self {
        EventSender {
            sender: self.sender.clone(),
            max_nps: self.max_nps.clone(),

            // Rough nps tracker is only used for very extreme spam situations,
            // so creating a new one when cloning shouldn't be an issue
            nps: RoughNpsTracker::new(),

            // Skipped notes is related to nps limiter, therefore it's also not cloned
            skipped_notes: [0; 128],
        }
    }
}

#[derive(Clone)]
pub struct RealtimeEventSender {
    senders: Vec<EventSender>,
}

impl RealtimeEventSender {
    fn new(
        senders: Vec<Sender<ChannelEvent>>,
        max_nps: Arc<ReadWriteAtomicU64>,
    ) -> RealtimeEventSender {
        RealtimeEventSender {
            senders: senders
                .into_iter()
                .map(|s| EventSender::new(max_nps.clone(), s))
                .collect(),
        }
    }

    pub fn send_event(&mut self, event: SynthEvent) {
        match event {
            SynthEvent::Channel(channel, event) => {
                self.senders[channel as usize].send(event);
            }
            SynthEvent::AllChannels(event) => {
                for sender in self.senders.iter_mut() {
                    sender.send(event.clone());
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
    pub fn open_with_default_output(channel_count: u32) -> Self {
        let host = cpal::default_host();

        let device = host
            .default_output_device()
            .expect("failed to find output device");
        println!("Output device: {}", device.name().unwrap());

        let config = device.default_output_config().unwrap();

        RealtimeSynth::open(channel_count, &device, config)
    }

    pub fn open(channel_count: u32, device: &Device, config: SupportedStreamConfig) -> Self {
        let mut channels = Vec::new();
        let mut senders = Vec::new();
        let mut command_senders = Vec::new();

        let sample_rate = config.sample_rate().0;
        let audio_channels = config.channels();

        let use_threadpool = false;

        let pool = if use_threadpool {
            Some(Arc::new(rayon::ThreadPoolBuilder::new().build().unwrap()))
        } else {
            None
        };

        let (output_sender, output_receiver) = bounded::<Vec<f32>>(channel_count as usize);

        for _ in 0u32..channel_count {
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
                    },
                    err_fn,
                )
                .unwrap();

            stream
        }

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => build_stream::<f32>(&device, config, buffered.clone()),
            cpal::SampleFormat::I16 => build_stream::<i16>(&device, config, buffered.clone()),
            cpal::SampleFormat::U16 => build_stream::<u16>(&device, config, buffered.clone()),
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
