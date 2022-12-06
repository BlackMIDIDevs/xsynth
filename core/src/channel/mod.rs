use std::{
    cell::RefCell,
    sync::{atomic::AtomicU64, Arc},
};

use crate::{
    effects::MultiPassFilter,
    helpers::{prepapre_cache_vec, sum_simd},
    voice::VoiceControlData,
    AudioStreamParams, SingleBorrowRefCell,
};

use soundfonts::FilterType;

use self::{
    key::KeyData,
    params::{VoiceChannelParams, VoiceChannelStatsReader},
};

use super::AudioPipe;

use atomic_refcell::AtomicRefCell;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use to_vec::ToVec;

mod channel_sf;
mod key;
mod params;
mod voice_buffer;
mod voice_spawner;

mod event;
pub use event::*;

struct Key {
    data: SingleBorrowRefCell<KeyData>,
    audio_cache: SingleBorrowRefCell<Vec<f32>>,
    event_cache: SingleBorrowRefCell<Vec<KeyNoteEvent>>,
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
    volume: f32, // 0.0 = silent, 1.0 = max volume
    pan: f32,    // 0.0 = left, 0.5 = center, 1.0 = right
    cutoff: Option<f32>,
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
            volume: 1.0,
            pan: 0.5,
            cutoff: None,
        }
    }
}

pub struct VoiceChannel {
    key_voices: Arc<Vec<Key>>,

    params: VoiceChannelParams,
    threadpool: Option<Arc<rayon::ThreadPool>>,

    /// The helper struct for keeping track of MIDI control event data
    control_event_data: RefCell<ControlEventData>,

    /// Processed control data, ready to feed to voices
    voice_control_data: AtomicRefCell<VoiceControlData>,

    // Effects
    cutoff: MultiPassFilter,
}

impl VoiceChannel {
    pub fn new(
        stream_params: AudioStreamParams,
        threadpool: Option<Arc<rayon::ThreadPool>>,
    ) -> VoiceChannel {
        fn fill_key_array<T, F: Fn(u8) -> T>(func: F) -> Vec<T> {
            let mut vec = Vec::with_capacity(128);
            for i in 0..128 {
                vec.push(func(i));
            }
            vec
        }

        let params = VoiceChannelParams::new(stream_params);
        let shared_voice_counter = params.stats.voice_counter.clone();

        VoiceChannel {
            params,
            key_voices: Arc::new(fill_key_array(|i| {
                Key::new(i, shared_voice_counter.clone())
            })),

            threadpool,

            control_event_data: RefCell::new(ControlEventData::new_defaults()),
            voice_control_data: AtomicRefCell::new(VoiceControlData::new_defaults()),

            cutoff: MultiPassFilter::new(
                FilterType::LowPass { passes: 2 },
                stream_params.channels.count(),
                20000.0,
                stream_params.sample_rate as f32,
            ),
        }
    }

    fn apply_channel_effects(&mut self, out: &mut [f32]) {
        let control = self.control_event_data.borrow();

        // Volume
        for sample in out.iter_mut() {
            *sample *= control.volume;
        }

        // Panning
        for sample in out.iter_mut().skip(0).step_by(2) {
            *sample *= (control.pan * 2f32).min(1.0);
        }
        for sample in out.iter_mut().skip(1).step_by(2) {
            *sample *= ((1.0 - control.pan) * 2f32).min(1.0);
        }

        // Cutoff
        if let Some(cutoff) = control.cutoff {
            self.cutoff.set_cutoff(cutoff);
            self.cutoff.cutoff_samples(out);
        }
    }

    fn push_key_events_and_render(&mut self, out: &mut [f32]) {
        fn render_for_key(
            key: &Key,
            len: usize,
            control: &VoiceControlData,
            params: &VoiceChannelParams,
        ) {
            let mut events = key.event_cache.borrow();

            let mut audio_cache = key.audio_cache.borrow();
            let mut data = key.data.borrow();

            for e in events.drain(..) {
                data.send_event(e, control, &params.channel_sf, params.layers);
            }

            prepapre_cache_vec(&mut audio_cache, len, 0.0);

            data.render_to(&mut audio_cache);
        }

        out.fill(0.0);
        match self.threadpool.as_ref() {
            Some(pool) => {
                let len = out.len();
                let key_voices = self.key_voices.clone();
                let control = self.voice_control_data.borrow();
                let params = &self.params;
                pool.install(|| {
                    key_voices.par_iter().for_each(move |key| {
                        render_for_key(key, len, &control, params);
                    });
                });

                for key in self.key_voices.iter() {
                    let key = key.audio_cache.borrow();
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
                    sum_simd(key, out);
                }
            }
        }

        self.apply_channel_effects(out);
    }

    fn propagate_voice_controls(&self) {
        let controls = self.voice_control_data.borrow();
        for key in self.key_voices.iter() {
            let mut data = key.data.borrow();
            data.process_controls(&controls);
        }
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
                0x07 => {
                    // Volume
                    let vol: f32 = value as f32 / 128.0;
                    self.control_event_data.borrow_mut().volume = vol
                }
                0x0A => {
                    // Pan
                    let pan: f32 = value as f32 / 128.0;
                    self.control_event_data.borrow_mut().pan = pan
                }
                0x40 => {
                    // Damper / Sustain
                    let damper = match value {
                        0..=63 => false,
                        64..=127 => true,
                        _ => false,
                    };

                    for key in self.key_voices.iter() {
                        key.data.borrow().set_damper(damper);
                    }
                }
                0x4A => {
                    // Cutoff
                    let cutoff = (value as f32 / 127.0) * 18000.0;
                    self.control_event_data.borrow_mut().cutoff = Some(cutoff)
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

    pub fn process_event(&mut self, event: ChannelEvent) {
        self.push_events_iter(std::iter::once(event));
    }

    pub fn push_events_iter<T: Iterator<Item = ChannelEvent>>(&mut self, iter: T) {
        let mut key_events = self
            .key_voices
            .iter()
            .map(|k| k.event_cache.borrow())
            .to_vec();
        for e in iter {
            match e {
                ChannelEvent::Audio(audio) => match audio {
                    ChannelAudioEvent::NoteOn { key, vel } => {
                        let ev = KeyNoteEvent::On(vel);
                        if let Some(events) = key_events.get_mut(key as usize) {
                            events.push(ev);
                        }
                    }
                    ChannelAudioEvent::NoteOff { key } => {
                        let ev = KeyNoteEvent::Off;
                        if let Some(events) = key_events.get_mut(key as usize) {
                            events.push(ev);
                        }
                    }
                    ChannelAudioEvent::AllNotesOff => {
                        let ev = KeyNoteEvent::AllOff;
                        for key in key_events.iter_mut() {
                            key.push(ev.clone());
                        }
                    }
                    ChannelAudioEvent::AllNotesKilled => {
                        let ev = KeyNoteEvent::AllKilled;
                        for key in key_events.iter_mut() {
                            key.push(ev.clone());
                        }
                    }
                    ChannelAudioEvent::Control(control) => {
                        self.process_control_event(control);
                    }
                },
                ChannelEvent::Config(config) => self.params.process_config_event(config),
            }
        }
    }

    pub fn get_channel_stats(&self) -> VoiceChannelStatsReader {
        let stats = self.params.stats.clone();
        VoiceChannelStatsReader::new(stats)
    }
}

impl AudioPipe for VoiceChannel {
    fn stream_params(&self) -> &AudioStreamParams {
        &self.params.constant.stream_params
    }

    fn read_samples_unchecked(&mut self, out: &mut [f32]) {
        self.push_key_events_and_render(out);
    }
}
