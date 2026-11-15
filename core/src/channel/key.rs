use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use crate::voice::PortamentoControl;

use super::{
    channel_sf::ChannelSoundfont,
    event::KeyNoteEvent,
    voice_buffer::{FilterBuses, VoiceBuffer},
    ChannelInitOptions, VoiceControlData,
};

pub struct KeyData {
    key: u8,
    voices: VoiceBuffer,
    filter_buses: FilterBuses,
    last_voice_count: usize,
    shared_voice_counter: Arc<AtomicU64>,
}

impl KeyData {
    pub fn new(
        key: u8,
        shared_voice_counter: Arc<AtomicU64>,
        options: ChannelInitOptions,
        channels: usize,
    ) -> KeyData {
        KeyData {
            key,
            voices: VoiceBuffer::new(options),
            filter_buses: FilterBuses::new(channels),
            last_voice_count: 0,
            shared_voice_counter,
        }
    }

    pub fn send_event(
        &mut self,
        event: KeyNoteEvent,
        control: &VoiceControlData,
        channel_sf: &ChannelSoundfont,
        max_layers: Option<usize>,
    ) {
        match event {
            KeyNoteEvent::On(vel) => {
                let voices = channel_sf.spawn_voices_attack(control, self.key, vel);
                self.voices.push_voices(voices, max_layers);
            }
            KeyNoteEvent::Off => {
                let vel = self.voices.release_next_voice();
                if let Some(vel) = vel {
                    let voices = channel_sf.spawn_voices_release(control, self.key, vel);
                    self.voices.push_voices(voices, max_layers);
                }
            }
            KeyNoteEvent::AllOff => {
                while let Some(vel) = self.voices.release_next_voice() {
                    let voices = channel_sf.spawn_voices_release(control, self.key, vel);
                    self.voices.push_voices(voices, max_layers);
                }
            }
            KeyNoteEvent::AllKilled => {
                self.voices.kill_all_voices();
            }
            KeyNoteEvent::OnGlide {
                vel,
                from_key,
                time,
            } => {
                let mut control = *control;
                control.portamento = Some(PortamentoControl { from_key, time });
                let voices = channel_sf.spawn_voices_attack(&control, self.key, vel);
                self.voices.push_voices(voices, max_layers);
            }
            KeyNoteEvent::CcTriggered(index) => {
                if let Some(trig) = channel_sf.cc_triggered().get(index as usize) {
                    let voice = trig.spawner.spawn_voice(control);
                    self.voices.push_voices(std::iter::once(voice), max_layers);
                }
            }
        }
    }

    pub fn set_sostenuto(&mut self, sostenuto: bool) {
        self.voices.set_sostenuto(sostenuto);
    }

    pub fn process_controls(&mut self, control: &VoiceControlData) {
        for voice in &mut self.voices.iter_voices_mut() {
            voice.process_controls(control);
        }
    }

    pub fn render_to(&mut self, out: &mut [f32]) {
        if self.has_voices() {
            self.voices.render_voices(out, &mut self.filter_buses);
        } else {
            self.filter_buses.reset();
        }

        // The counter is shared by every key of the channel, so only touch it
        // when the count changed.
        let voice_count = self.voices.voice_count();
        if voice_count > self.last_voice_count {
            self.shared_voice_counter.fetch_add(
                (voice_count - self.last_voice_count) as u64,
                Ordering::Relaxed,
            );
        } else if voice_count < self.last_voice_count {
            self.shared_voice_counter.fetch_sub(
                (self.last_voice_count - voice_count) as u64,
                Ordering::Relaxed,
            );
        }
        self.last_voice_count = voice_count;
    }

    pub fn voice_count(&self) -> usize {
        self.voices.voice_count()
    }

    pub fn has_voices(&self) -> bool {
        self.voices.has_voices()
    }

    pub fn set_damper(&mut self, damper: bool) {
        self.voices.set_damper(damper);
    }

    pub fn kill_by_exclusive_class(&mut self, class: u8) {
        self.voices.kill_by_exclusive_class(class);
    }
}
