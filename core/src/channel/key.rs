use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use super::{
    channel_sf::ChannelSoundfont, event::NoteEvent, voice_buffer::VoiceBuffer, VoiceControlData,
};

pub struct KeyData {
    key: u8,
    voices: VoiceBuffer,

    /// If a note on was skipped, then this helps track the note offs that should be ignored
    skipped_ons: VecDeque<bool>,

    last_voice_count: usize,
    shared_voice_counter: Arc<AtomicU64>,
}

impl KeyData {
    pub fn new(key: u8, shared_voice_counter: Arc<AtomicU64>) -> KeyData {
        KeyData {
            key,
            voices: VoiceBuffer::new(),
            last_voice_count: 0,
            skipped_ons: VecDeque::new(),
            shared_voice_counter,
        }
    }

    pub fn send_event(
        &mut self,
        event: NoteEvent,
        control: &VoiceControlData,
        channel_sf: &ChannelSoundfont,
        max_layers: Option<usize>,
    ) {
        match event {
            NoteEvent::On(vel) => {
                let voices = channel_sf.spawn_voices_attack(control, self.key, vel);
                let pushed = self.voices.push_voices(vel, voices, max_layers);
                self.skipped_ons.push_front(!pushed);
            }
            NoteEvent::Off => {
                let is_skipped = self.skipped_ons.pop_back();
                if is_skipped != Some(true) {
                    let vel = self.voices.release_next_voice();
                    if let Some(vel) = vel {
                        let voices = channel_sf.spawn_voices_release(control, self.key, vel);
                        self.voices.push_voices(vel, voices, max_layers);
                    }
                }
            }
        }
    }

    pub fn process_controls(&mut self, control: &VoiceControlData) {
        for voice in &mut self.voices.iter_voices_mut() {
            voice.process_controls(control);
        }
    }

    pub fn render_to(&mut self, out: &mut [f32]) {
        if !self.has_voices() {
            return;
        }
        for voice in &mut self.voices.iter_voices_mut() {
            voice.render_to(out);
        }
        self.voices.remove_ended_voices();

        let voice_count = self.voices.voice_count();
        let change = voice_count as i64 - self.last_voice_count as i64;
        if change < 0 {
            self.shared_voice_counter
                .fetch_sub((-change) as u64, Ordering::SeqCst);
        } else {
            self.shared_voice_counter
                .fetch_add(change as u64, Ordering::SeqCst);
        }
        self.last_voice_count = voice_count;
    }

    pub fn has_voices(&self) -> bool {
        self.voices.has_voices()
    }
}
