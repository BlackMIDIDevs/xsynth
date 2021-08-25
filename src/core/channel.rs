use std::sync::{atomic::AtomicU64, Arc, Mutex, RwLock};

use crate::{
    helpers::{prepapre_cache_vec, sum_simd},
    AudioStreamParams, SingleBorrowRefCell,
};

use self::{
    event::{ChannelEvent, NoteEvent},
    key::KeyData,
    params::{VoiceChannelConst, VoiceChannelParams, VoiceChannelStatsReader},
};

use super::{effects::VolumeLimiter, AudioPipe};

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use to_vec::ToVec;

mod channel_sf;
pub mod event;
mod key;
mod params;
pub mod voice;
mod voice_buffer;
mod voice_spawner;

#[derive(Clone)]
pub struct VoiceChannel {
    data: Arc<Mutex<VoiceChannelData>>,

    constants: VoiceChannelConst,
}

struct Key {
    data: SingleBorrowRefCell<KeyData>,
    audio_cache: SingleBorrowRefCell<Vec<f32>>,
    event_cache: SingleBorrowRefCell<Vec<NoteEvent>>,
}

impl Key {
    pub fn new(key: u8, shared_voice_counter: Arc<AtomicU64>) -> Self {
        Key {
            data: SingleBorrowRefCell::new(KeyData::new(key, shared_voice_counter)),
            audio_cache: SingleBorrowRefCell::new(Vec::new()),
            event_cache: SingleBorrowRefCell::new(Vec::new()),
        }
    }
}

struct VoiceChannelData {
    key_voices: Arc<Vec<Key>>,
    params: Arc<RwLock<VoiceChannelParams>>,

    threadpool: Option<Arc<rayon::ThreadPool>>,

    limiter: VolumeLimiter,
}

impl VoiceChannelData {
    pub fn new(
        sample_rate: u32,
        channels: u16,
        threadpool: Option<Arc<rayon::ThreadPool>>,
    ) -> VoiceChannelData {
        fn fill_key_array<T, F: Fn(u8) -> T>(func: F) -> Vec<T> {
            let mut vec = Vec::with_capacity(128);
            for i in 0..128 {
                vec.push(func(i));
            }
            vec
        }

        let params = VoiceChannelParams::new(sample_rate, channels);
        let shared_voice_counter = params.stats.voice_counter.clone();

        VoiceChannelData {
            params: Arc::new(RwLock::new(params)),
            key_voices: Arc::new(fill_key_array(|i| {
                Key::new(i, shared_voice_counter.clone())
            })),

            threadpool,

            limiter: VolumeLimiter::new(channels),
        }
    }

    fn push_key_events_and_render(&mut self, out: &mut [f32]) {
        fn render_for_key(key: &Key, len: usize, params: &Arc<RwLock<VoiceChannelParams>>) {
            let mut events = key.event_cache.borrow();

            let mut audio_cache = key.audio_cache.borrow();
            let mut data = key.data.borrow();

            let params = params.read().unwrap();
            for e in events.drain(..) {
                data.send_event(e, &params.channel_sf, params.layers as usize);
            }

            prepapre_cache_vec(&mut audio_cache, len, 0.0);

            data.render_to(&mut audio_cache);
        }

        out.fill(0.0);
        match self.threadpool.as_ref() {
            Some(pool) => {
                let len = out.len();
                let params = self.params.clone();
                let key_voices = self.key_voices.clone();
                pool.install(|| {
                    let params = params.clone();
                    key_voices.par_iter().for_each(move |key| {
                        render_for_key(key, len, &params);
                    });
                });

                for key in self.key_voices.iter() {
                    let key = &key.audio_cache.borrow();
                    sum_simd(&key, out);
                }
            }
            None => {
                let len = out.len();

                for key in self.key_voices.iter() {
                    render_for_key(key, len, &self.params);
                }

                for key in self.key_voices.iter() {
                    let key = &key.audio_cache.borrow();
                    sum_simd(&key, out);
                }
            }
        }

        self.limiter.limit(out);
    }
}

impl VoiceChannel {
    pub fn new(
        sample_rate: u32,
        channels: u16,
        threadpool: Option<Arc<rayon::ThreadPool>>,
    ) -> VoiceChannel {
        let data = VoiceChannelData::new(sample_rate, channels, threadpool);

        let constants = data.params.read().unwrap().constant.clone();

        VoiceChannel {
            data: Arc::new(Mutex::new(data)),
            constants,
        }
    }

    pub fn process_note_event(&self, key: u8, event: NoteEvent) {
        let data = self.data.lock().unwrap();
        let params = &data.params.read().unwrap();
        data.key_voices[key as usize].data.borrow().send_event(
            event,
            &params.channel_sf,
            params.layers as usize,
        );
    }

    pub fn process_event(&self, event: ChannelEvent) {
        match event {
            ChannelEvent::NoteOn { key, vel } => self.process_note_event(key, NoteEvent::On(vel)),
            ChannelEvent::NoteOff { key } => self.process_note_event(key, NoteEvent::Off),
        }
    }

    pub fn push_events_iter<T: Iterator<Item = ChannelEvent>>(&self, iter: T) {
        let data = self.data.lock().unwrap();
        for e in iter {
            let mut key_events = data
                .key_voices
                .iter()
                .map(|k| k.event_cache.borrow())
                .to_vec();
            match e {
                ChannelEvent::NoteOn { key, vel } => {
                    let ev = NoteEvent::On(vel);
                    key_events[key as usize].push(ev);
                }
                ChannelEvent::NoteOff { key } => {
                    let ev = NoteEvent::Off;
                    key_events[key as usize].push(ev);
                }
            }
        }
    }

    pub fn get_channel_stats(&self) -> VoiceChannelStatsReader {
        let data = self.data.lock().unwrap();
        let stats = data.params.read().unwrap().stats.clone();
        VoiceChannelStatsReader::new(stats.clone())
    }
}

impl AudioPipe for VoiceChannel {
    fn stream_params<'a>(&'a self) -> &'a AudioStreamParams {
        &self.constants.stream_params
    }

    fn read_samples_unchecked(&mut self, out: &mut [f32]) {
        let mut data = self.data.lock().unwrap();
        data.push_key_events_and_render(out);
    }
}
