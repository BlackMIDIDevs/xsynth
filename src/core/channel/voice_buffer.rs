use super::voice::Voice;
use std::{
    collections::VecDeque,
    ops::{Deref, DerefMut},
};

struct GroupVoice {
    pub id: usize,
    pub voice: Box<dyn Voice>,
}

impl Deref for GroupVoice {
    type Target = Box<dyn Voice>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.voice
    }
}

impl DerefMut for GroupVoice {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Box<(dyn Voice)> {
        &mut self.voice
    }
}

pub struct VoiceBuffer {
    id_counter: usize,
    buffer: VecDeque<GroupVoice>,
}

impl VoiceBuffer {
    pub fn new() -> Self {
        VoiceBuffer {
            id_counter: 0,
            buffer: VecDeque::new(),
        }
    }

    fn get_id(&mut self) -> usize {
        self.id_counter += 1;
        self.id_counter
    }

    fn pop_quietest_voice_group(&mut self) {
        if self.buffer.len() == 0 {
            return;
        }

        let mut quietest = 255u8;
        let mut quietest_index = 0;
        let mut quietest_id = 0;
        let mut count = 0;
        let mut releasing = false;
        for i in 0..self.buffer.len() {
            let voice = &self.buffer[i];
            let vel = voice.velocity();
            let voice_releasing = voice.is_releasing();
            if vel < quietest || (!releasing && voice_releasing) {
                quietest = vel;
                quietest_index = i;
                quietest_id = voice.id;
                count = 1;
                releasing = voice_releasing;
            } else if quietest_id == voice.id {
                count += 1;
            }
        }

        if count > 0 {
            self.buffer.drain(quietest_index..(quietest_index + count));
        }
    }

    /// Whether there is spare room or there are any voices in this buffer
    /// with a lower velocity that can be removed
    fn can_push_voices_with_velocity(&self, vel: u8, max_voices: Option<usize>) -> bool {
        if let Some(max_layers) = max_voices {
            if self.buffer.len() < max_layers {
                true
            } else {
                self.buffer.iter().any(|voice| {
                    voice.velocity() < vel || voice.is_releasing()
                });
                true
            }
        } else {
            true
        }
    }

    pub fn push_voices(
        &mut self,
        vel: u8,
        voices: impl Iterator<Item = Box<dyn Voice>>,
        max_voices: Option<usize>,
    ) -> bool {
        if self.can_push_voices_with_velocity(vel, max_voices) {
            let id = self.get_id();
            for voice in voices {
                self.buffer.push_back(GroupVoice { id, voice });
            }

            if let Some(max_voices) = max_voices {
                while self.buffer.len() > max_voices {
                    self.pop_quietest_voice_group();
                }
            }

            true
        } else {
            false
        }
    }

    pub fn release_next_voice(&mut self) -> Option<u8> {
        let mut id: Option<usize> = None;
        let mut vel = None;

        // Find the first non releasing voice, get its id and release all voices with that id
        for voice in self.buffer.iter_mut() {
            if voice.is_releasing() {
                continue;
            }

            if id.is_none() {
                id = Some(voice.id);
                vel = Some(voice.velocity())
            }

            if id != Some(voice.id) {
                break;
            }

            voice.signal_release();
        }

        vel
    }

    pub fn remove_ended_voices(&mut self) {
        let mut i = 0;
        while i < self.buffer.len() {
            if self.buffer[i].ended() {
                self.buffer.remove(i);
            } else {
                i += 1;
            }
        }
    }

    // pub fn iter_voices<'a>(&'a self) -> impl Iterator<Item = &Box<dyn Voice>> + 'a {
    //     self.buffer.iter().map(|group| &group.voice)
    // }

    pub fn iter_voices_mut<'a>(&'a mut self) -> impl Iterator<Item = &mut Box<dyn Voice>> + 'a {
        self.buffer.iter_mut().map(|group| &mut group.voice)
    }

    pub fn has_voices(&self) -> bool {
        !self.buffer.is_empty()
    }

    pub fn voice_count(&self) -> usize {
        self.buffer.len()
    }
}
