use std::{collections::VecDeque, sync::atomic::Ordering};

use super::{event::NoteEvent, params::VoiceChannelParams, voice::Voice};

pub struct KeyData {
    key: u8,
    voices: VecDeque<Voice>,
}

impl KeyData {
    pub fn new(key: u8) -> KeyData {
        KeyData {
            key,
            voices: VecDeque::new(),
        }
    }

    pub fn send_event(&mut self, event: NoteEvent, params: &VoiceChannelParams) {
        let mut change = 0i32;
        match event {
            NoteEvent::On(vel) => {
                while self.voices.len() >= params.layers as usize {
                    change -= 1;
                    self.voices.pop_back();
                }
                change += 1;
                self.voices
                    .push_front(Voice::spawn(self.key, vel, params.constant.sample_rate))
            }
            NoteEvent::Off => {
                change -= 1;
                self.voices.pop_back();
                // for voice in &mut self.voices {

                // }
            }
        }
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
        for voice in &mut self.voices {
            voice.render_to(out);
        }
    }
}
