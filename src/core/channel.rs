use std::{
    borrow::BorrowMut,
    sync::{atomic::AtomicU64, Arc, Mutex, RwLock},
};

use self::{
    event::{ChannelEvent, NoteEvent},
    key::KeyData,
    params::VoiceChannelParams,
};

use super::AudioPipe;

use atomic_refcell::AtomicRefCell;

mod event;
mod key;
mod params;
mod voice;

pub struct VoiceChannel {
    data: Arc<Mutex<VoiceChannelData>>,
    sample_rate: u32,
    channels: u16,
}

struct VoiceChannelData {
    key_voices: Vec<AtomicRefCell<KeyData>>,
    params: Arc<RwLock<VoiceChannelParams>>,

    key_event_caches: Vec<AtomicRefCell<Vec<NoteEvent>>>,
    output_caches: Vec<AtomicRefCell<Vec<Vec<f32>>>>,

    threadpool: Option<rayon::ThreadPool>,
}

impl VoiceChannelData {
    pub fn new(
        sample_rate: u32,
        channels: u16,
        threadpool: Option<rayon::ThreadPool>,
    ) -> VoiceChannelData {
        fn fill_key_array<T, F: Fn(u8) -> T>(func: F) -> Vec<AtomicRefCell<T>> {
            let mut vec = Vec::with_capacity(128);
            for i in 0..128 {
                vec.push(AtomicRefCell::new(func(i)));
            }
            vec
        }

        let params = Arc::new(RwLock::new(VoiceChannelParams::new(sample_rate, channels)));

        VoiceChannelData {
            params,
            key_voices: fill_key_array(|i| KeyData::new(i)),
            output_caches: fill_key_array(|_| Vec::new()),
            key_event_caches: fill_key_array(|_| Vec::new()),

            threadpool,
        }
    }
}

impl VoiceChannel {
    pub fn new(
        sample_rate: u32,
        channels: u16,
        threadpool: Option<rayon::ThreadPool>,
    ) -> VoiceChannel {
        VoiceChannel {
            data: Arc::new(Mutex::new(VoiceChannelData::new(
                sample_rate,
                channels,
                threadpool,
            ))),
            sample_rate,
            channels,
        }
    }

    pub fn process_note_event(&self, key: u8, event: NoteEvent) {
        let data = self.data.lock().unwrap();
        data.key_voices[key as usize]
            .borrow_mut()
            .send_event(event, &data.params.read().unwrap());
    }

    pub fn process_event(&self, event: ChannelEvent) {
        match event {
            ChannelEvent::NoteOn { key, vel } => self.process_note_event(key, NoteEvent::On(vel)),
            ChannelEvent::NoteOff { key } => self.process_note_event(key, NoteEvent::Off),
        }
    }
}

impl AudioPipe for VoiceChannel {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn read_samples_unchecked(&mut self, out: &mut [f32]) {
        let data = self.data.lock().unwrap();

        out.fill(0.0);
        for k in &data.key_voices {
            let mut k = k.borrow_mut();
            k.render_to(out);
        }
    }
}
