use super::voice::Voice;
use std::{collections::VecDeque, ops::{Deref, DerefMut}};

struct GroupVoice {
    pub id: u64,
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
    id_counter: u64,
    buffer: VecDeque<GroupVoice>,
}

impl VoiceBuffer {
    pub fn new() -> Self {
        VoiceBuffer {
            id_counter: 0,
            buffer: VecDeque::new(),
        }
    }

    fn get_id(&mut self) -> u64 {
        self.id_counter += 1;
        self.id_counter
    }

    fn pop_last_voice_group(&mut self) {
        if let Some(voice) = self.buffer.back() {
            let id = voice.id;
            loop {
                self.buffer.pop_back();
                if let Some(voice) = self.buffer.back() {
                    if voice.id != id {
                        break;
                    }
                }
            }
        }
    }

    pub fn push_voices(&mut self, voices: impl Iterator<Item = Box<dyn Voice>>, max_voices: usize) {
        let id = self.get_id();
        for voice in voices {
            self.buffer.push_front(GroupVoice { id, voice });
        }

        while self.buffer.len() > max_voices {
            self.pop_last_voice_group();
        }
    }

    pub fn release_next_voice(&mut self) {
        let mut id: Option<u64> = None;

        // Find the first non releasing voice, get its id and release all voices with that id
        for voice in self.buffer.iter_mut() {
            if voice.is_releasing() {
                continue;
            }
            
            if id.is_none() {
                id = Some(voice.id);
            }
            
            if id != Some(voice.id) {
                break;
            }

            voice.signal_release();
        }
    }

    pub fn iter_voices<'a>(&'a self) -> impl Iterator<Item = &Box<dyn Voice>> + 'a {
        self.buffer.iter().map(|group| &group.voice)
    }

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
