use std::sync::Arc;

use crate::core::soundfont::SoundfontBase;

#[derive(Debug)]
pub enum NoteEvent {
    On(u8),
    Off,
}

#[derive(Debug, Clone)]
pub enum ChannelEvent {
    NoteOn { key: u8, vel: u8 },
    NoteOff { key: u8 },
    Control(ControlEvent),

    SetSoundfonts(Vec<Arc<dyn SoundfontBase>>),
}

#[derive(Debug, Clone)]
pub enum ControlEvent {
    Raw(u8, u8),

    /// The pitch bend strength, in tones
    PitchBendSensitivity(f32),

    /// The pitch bend value, between -1 and 1
    PitchBendValue(f32),

    /// The pitch bend, product of value * sensitivity
    PitchBend(f32),
}
