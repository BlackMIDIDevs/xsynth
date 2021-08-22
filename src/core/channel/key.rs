use std::{sync::atomic::Ordering};

use super::{event::NoteEvent, params::VoiceChannelParams, voice_buffer::VoiceBuffer};

pub struct KeyData {
    key: u8,
    voices: VoiceBuffer,
}

impl KeyData {
    pub fn new(key: u8) -> KeyData {
        KeyData {
            key,
            voices: VoiceBuffer::new(),
        }
    }

    pub fn send_event(&mut self, event: NoteEvent, params: &VoiceChannelParams) {
        let initial_count = self.voices.voice_count();
        match event {
            NoteEvent::On(vel) => {
                let voices = params.channel_sf.spawn_voices_attack(self.key, vel);
                self.voices.push_voices(voices, params.layers as usize);
            }
            NoteEvent::Off => {
                self.voices.release_next_voice();
                let voices = params.channel_sf.spawn_voices_release(self.key);
                self.voices.push_voices(voices, params.layers as usize);
            }
        }
        let change = self.voices.voice_count() as i32 - initial_count as i32;
        if change < 0 {
            params
                .stats
                .voice_counter
                .fetch_sub((-change) as u64, Ordering::SeqCst);
        } else {
            params
                .stats
                .voice_counter
                .fetch_add(change as u64, Ordering::SeqCst);
        }
    }

    pub fn render_to(&mut self, out: &mut [f32]) {
        for voice in &mut self.voices.iter_voices_mut() {
            voice.render_to(out);
        }
    }

    pub fn has_voices(&self) -> bool {
        self.voices.has_voices()
    }
}
