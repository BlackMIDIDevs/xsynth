use crate::voice::Voice;
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
    damper: bool, // false = pedal up, true = pedal down
    held_by_damper: Vec<usize>,
}

impl VoiceBuffer {
    pub fn new() -> Self {
        VoiceBuffer {
            id_counter: 0,
            buffer: VecDeque::new(),
            damper: false,
            held_by_damper: Vec::new(),
        }
    }

    fn get_id(&mut self) -> usize {
        self.id_counter += 1;
        self.id_counter
    }

    /// Pops the quietest voice group. Multiple voices can be part of the same group
    /// based on their ID (e.g. a note and a hammer playing at the same time for a note on event)
    fn pop_quietest_voice_group(&mut self, reference_vel: u8, ignored_id: usize) {
        if self.buffer.len() == 0 {
            return;
        }

        let mut quietest = reference_vel;
        let mut quietest_index = 0;
        let mut quietest_id = 0;
        let mut count = 0;
        for i in 0..self.buffer.len() {
            let voice = &self.buffer[i];
            if voice.id == ignored_id {
                continue;
            }
            let vel = voice.velocity();
            if quietest_id == voice.id {
                count += 1;
            } else if vel < quietest || i == 0 {
                quietest = vel;
                quietest_index = i;
                quietest_id = voice.id;
                count = 1;
            }
        }

        if count > 0 {
            self.buffer.drain(quietest_index..(quietest_index + count));
        }
    }

    pub fn kill_all_voices(&mut self) {
        self.buffer.clear();
        self.id_counter = 0;
    }

    /// Pushes a new set of voices for a single note on event. Multiple voices can be part of the same group
    /// based on their ID (e.g. a note and a hammer playing at the same time for a note on event)
    pub fn push_voices(
        &mut self,
        vel: u8,
        voices: impl Iterator<Item = Box<dyn Voice>>,
        max_voices: Option<usize>,
    ) {
        let id = self.get_id();
        for voice in voices {
            self.buffer.push_back(GroupVoice { id, voice });
        }

        if let Some(max_voices) = max_voices {
            while self.buffer.len() > max_voices {
                self.pop_quietest_voice_group(vel, id);
            }
        }
    }

    /// Releases the next voice, and all subsequent voices that have the same ID.
    pub fn release_next_voice(&mut self) -> Option<u8> {
        let mut id: Option<usize> = None;
        let mut vel = None;

        // Find the first non releasing voice, get its id and release all voices with that id
        for voice in self.buffer.iter_mut() {
            if self.damper {
                self.held_by_damper.push(voice.id.clone());
            } else {
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

                for v in &mut self.held_by_damper {
                    if v == &mut voice.id {
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
                }
                self.held_by_damper.clear();
            }
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

    pub fn set_damper(&mut self, damper: bool) {
        self.damper = damper;
    }
}
