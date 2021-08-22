use std::{ops::Deref, sync::Arc};

use crate::core::soundfont::SoundfontBase;

use super::voice_spawner::VoiceSpawnerMatrix;

pub struct ChannelSoundfont {
    soundfonts: Vec<Arc<dyn SoundfontBase>>,
    matrix: VoiceSpawnerMatrix,
}

impl Deref for ChannelSoundfont {
    type Target = VoiceSpawnerMatrix;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.matrix
    }
}

impl ChannelSoundfont {
    pub fn new() -> Self {
        ChannelSoundfont {
            soundfonts: Vec::new(),
            matrix: VoiceSpawnerMatrix::new(),
        }
    }

    pub fn set_soundfonts(&mut self, soundfonts: Vec<Arc<dyn SoundfontBase>>) {
        self.soundfonts = soundfonts;
    }
}