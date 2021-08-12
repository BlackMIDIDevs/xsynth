use std::{collections::VecDeque, sync::atomic::Ordering};

use super::{event::NoteEvent, params::VoiceChannelParams, voice::Voice};

pub struct KeyData {
    key: u8,
    voices: VecDeque<Voice>,
    output_cache: Vec<f32>,
}

impl KeyData {
    pub fn new(key: u8) -> KeyData {
        KeyData {
            key,
            voices: VecDeque::new(),
            output_cache: Vec::new(),
        }
    }

    pub fn send_event(&mut self, event: NoteEvent, params: &VoiceChannelParams) {
        let mut change = 0;
        match event {
            NoteEvent::On(vel) => {
                while self.voices.len() >= params.layers as usize {
                    change -= 1;
                    self.voices.pop_back();
                }
                change += 1;
                self.voices
                    .push_front(Voice::spawn(self.key, vel, params.sample_rate))
            }
            NoteEvent::Off => {
                change -= 1;
                self.voices.pop_back();
                // for voice in &mut self.voices {

                // }
            }
        }
        params
            .stats
            .voice_counter
            .fetch_add(change, Ordering::SeqCst);
    }

    pub fn render_to(&mut self, out: &mut [f32]) {
        for voice in &mut self.voices {
            voice.render_to(out);
        }
    }
}
