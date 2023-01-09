use std::sync::{atomic::AtomicU64, Arc};

use crate::{
    effects::MultiChannelBiQuad,
    helpers::{prepapre_cache_vec, sum_simd},
    voice::VoiceControlData,
    AudioStreamParams,
};

use soundfonts::FilterType;

use self::{
    key::KeyData,
    params::{VoiceChannelParams, VoiceChannelStatsReader},
};

use super::AudioPipe;

use rayon::prelude::*;

mod channel_sf;
mod key;
mod params;
mod voice_buffer;
mod voice_spawner;

mod event;
pub use event::*;

struct Key {
    data: KeyData,
    audio_cache: Vec<f32>,
    event_cache: Vec<KeyNoteEvent>,
}

impl Key {
    pub fn new(key: u8, shared_voice_counter: Arc<AtomicU64>, options: ChannelInitOptions) -> Self {
        Key {
            data: KeyData::new(key, shared_voice_counter, options),
            audio_cache: Vec::new(),
            event_cache: Vec::new(),
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
    expression: f32,
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
            expression: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ChannelInitOptions {
    pub fade_out_killing: bool,
}

impl Default for ChannelInitOptions {
    fn default() -> Self {
        Self {
            fade_out_killing: true,
        }
    }
}

pub struct VoiceChannel {
    key_voices: Vec<Key>,

    params: VoiceChannelParams,
    threadpool: Option<Arc<rayon::ThreadPool>>,

    /// The helper struct for keeping track of MIDI control event data
    control_event_data: ControlEventData,

    /// Processed control data, ready to feed to voices
    voice_control_data: VoiceControlData,

    // Effects
    cutoff: MultiChannelBiQuad,
}

impl VoiceChannel {
    pub fn new(
        options: ChannelInitOptions,
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
            key_voices: fill_key_array(|i| Key::new(i, shared_voice_counter.clone(), options)),

            threadpool,

            control_event_data: ControlEventData::new_defaults(),
            voice_control_data: VoiceControlData::new_defaults(),

            cutoff: MultiChannelBiQuad::new(
                stream_params.channels.count() as usize,
                FilterType::LowPass,
                20000.0,
                stream_params.sample_rate as f32,
            ),
        }
    }

    fn apply_channel_effects(&mut self, out: &mut [f32]) {
        let control = &self.control_event_data;

        // Volume
        for sample in out.iter_mut() {
            *sample *= control.volume * control.expression;
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
            self.cutoff.set_filter_type(FilterType::LowPass, cutoff);
            self.cutoff.process(out);
        }
    }

    fn push_key_events_and_render(&mut self, out: &mut [f32]) {
        fn render_for_key(
            key: &mut Key,
            len: usize,
            control: &VoiceControlData,
            params: &VoiceChannelParams,
        ) {
            for e in key.event_cache.drain(..) {
                key.data
                    .send_event(e, control, &params.channel_sf, params.layers);
            }

            prepapre_cache_vec(&mut key.audio_cache, len, 0.0);

            key.data.render_to(&mut key.audio_cache);
        }

        out.fill(0.0);
        match self.threadpool.as_ref() {
            Some(pool) => {
                let len = out.len();
                let key_voices = &mut self.key_voices;
                let params = &self.params;
                let control_data = &self.voice_control_data;
                pool.install(|| {
                    key_voices.par_iter_mut().for_each(move |key| {
                        render_for_key(key, len, control_data, params);
                    });
                });

                for key in self.key_voices.iter() {
                    sum_simd(&key.audio_cache, out);
                }
            }
            None => {
                let len = out.len();

                for key in self.key_voices.iter_mut() {
                    render_for_key(key, len, &self.voice_control_data, &self.params);
                }

                for key in self.key_voices.iter() {
                    sum_simd(&key.audio_cache, out);
                }
            }
        }

        self.apply_channel_effects(out);
    }

    fn propagate_voice_controls(&mut self) {
        for key in self.key_voices.iter_mut() {
            key.data.process_controls(&self.voice_control_data);
        }
    }

    pub fn process_control_event(&mut self, event: ControlEvent) {
        match event {
            ControlEvent::Raw(controller, value) => match controller {
                0x64 => {
                    self.control_event_data.selected_lsb = value as i8;
                }
                0x65 => {
                    self.control_event_data.selected_msb = value as i8;
                }
                0x06 | 0x26 => {
                    let (lsb, msb) = {
                        let data = &self.control_event_data;
                        (data.selected_lsb, data.selected_msb)
                    };
                    if lsb == 0 && msb == 0 {
                        match controller {
                            0x06 => self.control_event_data.pitch_bend_sensitivity_msb = value,
                            0x26 => self.control_event_data.pitch_bend_sensitivity_lsb = value,
                            _ => (),
                        }

                        let sensitivity = {
                            let data = &self.control_event_data;
                            (data.pitch_bend_sensitivity_msb as f32)
                                + (data.pitch_bend_sensitivity_lsb as f32) / 100.0
                        };

                        self.process_control_event(ControlEvent::PitchBendSensitivity(sensitivity))
                    }
                }
                0x07 => {
                    // Volume
                    let vol: f32 = value as f32 / 128.0;
                    self.control_event_data.volume = vol
                }
                0x0A => {
                    // Pan
                    let pan: f32 = value as f32 / 128.0;
                    self.control_event_data.pan = pan
                }
                0x0B => {
                    // Expression
                    let expr = value as f32 / 128.0;
                    self.control_event_data.expression = expr
                }
                0x40 => {
                    // Damper / Sustain
                    let damper = match value {
                        0..=63 => false,
                        64..=127 => true,
                        _ => false,
                    };

                    for key in self.key_voices.iter_mut() {
                        key.data.set_damper(damper);
                    }
                }
                0x48 => {
                    // Release
                    self.voice_control_data.envelope.release = Some(value);
                    self.propagate_voice_controls();
                }
                0x49 => {
                    // Attack
                    self.voice_control_data.envelope.attack = Some(value);
                    self.propagate_voice_controls();
                }
                0x4A => {
                    // Cutoff
                    if value < 64 {
                        let cutoff = (value as f32 / 64.0).powi(2) * 20000.0 + 100.0;
                        self.control_event_data.cutoff = Some(cutoff);
                    } else {
                        self.control_event_data.cutoff = None;
                    }
                }
                0x78 => {
                    // All Sounds Off
                    if value == 0 {
                        self.process_event(ChannelEvent::Audio(ChannelAudioEvent::AllNotesKilled));
                    }
                }
                0x79 => {
                    // Reset All Controllers
                    if value == 0 {
                        self.reset_control();
                    }
                }
                0x7B => {
                    // All Notes Off
                    if value == 0 {
                        self.process_event(ChannelEvent::Audio(ChannelAudioEvent::AllNotesOff));
                    }
                }
                _ => {}
            },
            ControlEvent::PitchBendSensitivity(sensitivity) => {
                let pitch_bend = {
                    let data = &mut self.control_event_data;
                    data.pitch_bend_sensitivity = sensitivity;
                    data.pitch_bend_sensitivity * data.pitch_bend_value
                };
                self.process_control_event(ControlEvent::PitchBend(pitch_bend));
            }
            ControlEvent::PitchBendValue(value) => {
                let pitch_bend = {
                    let data = &mut self.control_event_data;
                    data.pitch_bend_value = value;
                    data.pitch_bend_sensitivity * data.pitch_bend_value
                };
                self.process_control_event(ControlEvent::PitchBend(pitch_bend));
            }
            ControlEvent::PitchBend(value) => {
                let multiplier = 2.0f32.powf(value / 12.0);
                self.voice_control_data.voice_pitch_multiplier = multiplier;
                self.propagate_voice_controls();
            }
        }
    }

    pub fn process_event(&mut self, event: ChannelEvent) {
        self.push_events_iter(std::iter::once(event));
    }

    pub fn push_events_iter<T: Iterator<Item = ChannelEvent>>(&mut self, iter: T) {
        for e in iter {
            match e {
                ChannelEvent::Audio(audio) => match audio {
                    ChannelAudioEvent::NoteOn { key, vel } => {
                        if let Some(key) = self.key_voices.get_mut(key as usize) {
                            let ev = KeyNoteEvent::On(vel);
                            key.event_cache.push(ev);
                        }
                    }
                    ChannelAudioEvent::NoteOff { key } => {
                        if let Some(key) = self.key_voices.get_mut(key as usize) {
                            let ev = KeyNoteEvent::Off;
                            key.event_cache.push(ev);
                        }
                    }
                    ChannelAudioEvent::AllNotesOff => {
                        for key in self.key_voices.iter_mut() {
                            let ev = KeyNoteEvent::AllOff;
                            key.event_cache.push(ev);
                        }
                    }
                    ChannelAudioEvent::AllNotesKilled => {
                        for key in self.key_voices.iter_mut() {
                            let ev = KeyNoteEvent::AllKilled;
                            key.event_cache.push(ev);
                        }
                    }
                    ChannelAudioEvent::ResetControl => {
                        self.reset_control();
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

    fn reset_control(&mut self) {
        self.control_event_data = ControlEventData::new_defaults();
        self.voice_control_data = VoiceControlData::new_defaults();
        self.propagate_voice_controls();

        self.control_event_data.cutoff = None;

        for key in self.key_voices.iter_mut() {
            key.data.set_damper(false);
        }
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
