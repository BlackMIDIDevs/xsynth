use std::sync::{Arc, Mutex, RwLock};

use crate::helpers::sum_simd;

use self::{
    event::{ChannelEvent, NoteEvent},
    key::KeyData,
    params::{VoiceChannelConst, VoiceChannelParams},
};

use super::AudioPipe;

use atomic_refcell::AtomicRefCell;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use to_vec::ToVec;

pub mod event;
mod key;
mod params;
mod voice;

#[derive(Clone)]
pub struct VoiceChannel {
    data: Arc<Mutex<VoiceChannelData>>,

    constants: VoiceChannelConst,
}

struct Key {
    data: AtomicRefCell<KeyData>,
    audio_cache: AtomicRefCell<Vec<f32>>,
    event_cache: AtomicRefCell<Vec<NoteEvent>>,
}

impl Key {
    pub fn new(key: u8) -> Self {
        Key {
            data: AtomicRefCell::new(KeyData::new(key)),
            audio_cache: AtomicRefCell::new(Vec::new()),
            event_cache: AtomicRefCell::new(Vec::new()),
        }
    }
}

struct VoiceChannelData {
    key_voices: Arc<Vec<Key>>,
    params: Arc<RwLock<VoiceChannelParams>>,

    threadpool: Option<Arc<rayon::ThreadPool>>,
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

        let params = Arc::new(RwLock::new(VoiceChannelParams::new(sample_rate, channels)));

        VoiceChannelData {
            params,
            key_voices: Arc::new(fill_key_array(|i| Key::new(i))),

            threadpool,
        }
    }

    fn push_key_events_and_render(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        match self.threadpool.as_ref() {
            Some(pool) => {
                let len = out.len();
                let params = self.params.clone();
                let key_voices = self.key_voices.clone();
                pool.install(|| {
                    let params = params.clone();
                    key_voices.par_iter().for_each(move |key| {
                        let mut events = key.event_cache.borrow_mut();

                        let mut audio_cache = key.audio_cache.borrow_mut();
                        let mut data = key.data.borrow_mut();

                        let params = params.read().unwrap();
                        for e in events.drain(..) {
                            data.send_event(e, &params);
                        }

                        audio_cache.clear();
                        audio_cache.reserve(len);
                        for _ in 0..len {
                            audio_cache.push(0.0);
                        }

                        data.render_to(&mut audio_cache);
                    });
                });

                for key in self.key_voices.iter() {
                    let key = &key.audio_cache.borrow();
                    sum_simd(&key, out);
                }
            }
            None => {
                //TODO: Make this one actually align to what the multithreaded one does
                todo!();
                // for i in 0..self.key_voices.len() {
                //     let key_data = &self.key_voices[i];
                //     let k = &mut key_data.data.borrow_mut();
                //     k.render_to(out);
                // }
            }
        }
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
        data.key_voices[key as usize]
            .data
            .borrow_mut()
            .send_event(event, &data.params.read().unwrap());
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
                .map(|k| k.event_cache.borrow_mut())
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
}

impl AudioPipe for VoiceChannel {
    fn sample_rate(&self) -> u32 {
        self.constants.sample_rate
    }

    fn channels(&self) -> u16 {
        self.constants.channels
    }

    fn read_samples_unchecked(&mut self, out: &mut [f32]) {
        let mut data = self.data.lock().unwrap();
        data.push_key_events_and_render(out);
    }
}
