use std::{
    cell::RefCell,
    sync::{atomic::AtomicU64, Arc, Mutex, RwLock},
};

use crate::{
    helpers::{prepapre_cache_vec, sum_simd},
    AudioStreamParams, SingleBorrowRefCell,
};

use self::{
    event::{ChannelEvent, ControlEvent, NoteEvent},
    key::KeyData,
    params::{VoiceChannelConst, VoiceChannelParams, VoiceChannelStatsReader},
};

use super::{effects::VolumeLimiter, soundfont::SoundfontBase, AudioPipe};

use atomic_refcell::AtomicRefCell;
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

struct ControlEventData {
    selected_lsb: i8,
    selected_msb: i8,
    pitch_bend_sensitivity_lsb: u8,
    pitch_bend_sensitivity_msb: u8,
    pitch_bend_sensitivity: f32,
    pitch_bend_value: f32,
}

impl ControlEventData {
    pub fn new_defaults() -> Self {
        ControlEventData {
            selected_lsb: -1,
            selected_msb: -1,
            pitch_bend_sensitivity_lsb: 0,
            pitch_bend_sensitivity_msb: 2,
            pitch_bend_sensitivity: 2.0,
            pitch_bend_value: 0.0,
        }
    }
}

pub struct VoiceControlData {
    pub voice_pitch_multiplier: f32,
}

impl VoiceControlData {
    pub fn new_defaults() -> Self {
        VoiceControlData {
            voice_pitch_multiplier: 1.0,
        }
    }
}

struct VoiceChannelData {
    key_voices: Arc<Vec<Key>>,
    params: Arc<RwLock<VoiceChannelParams>>,

    threadpool: Option<Arc<rayon::ThreadPool>>,

    /// The helper struct for keeping track of MIDI control event data
    control_event_data: RefCell<ControlEventData>,

    /// Processed control data, ready to feed to voices
    voice_control_data: AtomicRefCell<VoiceControlData>,

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

            control_event_data: RefCell::new(ControlEventData::new_defaults()),
            voice_control_data: AtomicRefCell::new(VoiceControlData::new_defaults()),

            limiter: VolumeLimiter::new(channels),
        }
    }

    fn push_key_events_and_render(&mut self, out: &mut [f32]) {
        fn render_for_key(
            key: &Key,
            len: usize,
            control: &VoiceControlData,
            params: &Arc<RwLock<VoiceChannelParams>>,
        ) {
            let mut events = key.event_cache.borrow();

            let mut audio_cache = key.audio_cache.borrow();
            let mut data = key.data.borrow();

            let params = params.read().unwrap();
            for e in events.drain(..) {
                data.send_event(e, control, &params.channel_sf, params.layers as usize);
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
                let control = self.voice_control_data.borrow();
                pool.install(|| {
                    let params = params.clone();
                    key_voices.par_iter().for_each(move |key| {
                        render_for_key(key, len, &control, &params);
                    });
                });

                for key in self.key_voices.iter() {
                    let key = &key.audio_cache.borrow();
                    sum_simd(&key, out);
                }
            }
            None => {
                let len = out.len();

                let control = self.voice_control_data.borrow();
                for key in self.key_voices.iter() {
                    render_for_key(key, len, &control, &self.params);
                }

                for key in self.key_voices.iter() {
                    let key = &key.audio_cache.borrow();
                    sum_simd(&key, out);
                }
            }
        }

        self.limiter.limit(out);
    }

    fn propagate_voice_controls(&self) {
        let controls = self.voice_control_data.borrow();
        for key in self.key_voices.iter() {
            let mut data = key.data.borrow();
            data.process_controls(&controls);
        }
    }

    pub fn set_soundfonts(&self, soundfonts: Vec<Arc<dyn SoundfontBase>>) {
        self.params
            .write()
            .unwrap()
            .channel_sf
            .set_soundfonts(soundfonts)
    }

    pub fn process_control_event(&self, event: ControlEvent) {
        match event {
            ControlEvent::Raw(controller, value) => match controller {
                0x64 => {
                    self.control_event_data.borrow_mut().selected_lsb = value as i8;
                }
                0x65 => {
                    self.control_event_data.borrow_mut().selected_msb = value as i8;
                }
                0x06 | 0x26 => {
                    let (lsb, msb) = {
                        let data = self.control_event_data.borrow();
                        (data.selected_lsb, data.selected_msb)
                    };
                    if lsb == 0 && msb == 0 {
                        match controller {
                            0x06 => {
                                self.control_event_data
                                    .borrow_mut()
                                    .pitch_bend_sensitivity_msb = value
                            }
                            0x26 => {
                                self.control_event_data
                                    .borrow_mut()
                                    .pitch_bend_sensitivity_lsb = value
                            }
                            _ => (),
                        }

                        let sensitivity = {
                            let data = self.control_event_data.borrow();
                            (data.pitch_bend_sensitivity_msb as f32)
                                + (data.pitch_bend_sensitivity_lsb as f32) / 100.0
                        };

                        self.process_control_event(ControlEvent::PitchBendSensitivity(sensitivity))
                    }
                }
                _ => {}
            },
            ControlEvent::PitchBendSensitivity(sensitivity) => {
                let pitch_bend = {
                    let mut data = self.control_event_data.borrow_mut();
                    data.pitch_bend_sensitivity = sensitivity;
                    data.pitch_bend_sensitivity * data.pitch_bend_value
                };
                self.process_control_event(ControlEvent::PitchBend(pitch_bend));
            }
            ControlEvent::PitchBendValue(value) => {
                let pitch_bend = {
                    let mut data = self.control_event_data.borrow_mut();
                    data.pitch_bend_value = value;
                    data.pitch_bend_sensitivity * data.pitch_bend_value
                };
                self.process_control_event(ControlEvent::PitchBend(pitch_bend));
            }
            ControlEvent::PitchBend(value) => {
                let multiplier = 2.0f32.powf(value / 12.0);
                self.voice_control_data.borrow_mut().voice_pitch_multiplier = multiplier;
                self.propagate_voice_controls();
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

    pub fn process_event(&self, event: ChannelEvent) {
        self.push_events_iter(std::iter::once(event));
    }

    pub fn push_events_iter<T: Iterator<Item = ChannelEvent>>(&self, iter: T) {
        let data = self.data.lock().unwrap();
        let mut key_events = data
            .key_voices
            .iter()
            .map(|k| k.event_cache.borrow())
            .to_vec();
        for e in iter {
            match e {
                ChannelEvent::NoteOn { key, vel } => {
                    let ev = NoteEvent::On(vel);
                    key_events[key as usize].push(ev);
                }
                ChannelEvent::NoteOff { key } => {
                    let ev = NoteEvent::Off;
                    key_events[key as usize].push(ev);
                }
                ChannelEvent::Control(control) => {
                    data.process_control_event(control);
                }
                ChannelEvent::SetSoundfonts(soundfonts) => data.set_soundfonts(soundfonts),
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
