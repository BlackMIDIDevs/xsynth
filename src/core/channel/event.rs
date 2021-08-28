use std::sync::Arc;

use crate::core::soundfont::SoundfontBase;

#[derive(Debug)]
pub enum NoteEvent {
    On(u8),
    Off,
}

#[derive(Debug)]
pub enum ChannelEvent {
    NoteOn { key: u8, vel: u8 },
    NoteOff { key: u8 },
    SetSoundfonts(Vec<Arc<dyn SoundfontBase>>),
}
