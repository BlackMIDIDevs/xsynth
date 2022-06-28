use std::sync::Arc;

use crate::soundfont::SoundfontBase;

#[derive(Debug, Clone)]
pub enum KeyNoteEvent {
    /// Starts a new note voice with a velocity
    On(u8),
    /// Signals off to a note voice
    Off,
    /// Signals off to all note voices
    AllOff,
    /// Kills all note voices without decay
    AllKilled,
}

#[derive(Debug, Clone)]
pub enum ChannelConfigEvent {
    /// Sets the soundfonts for the channel
    SetSoundfonts(Vec<Arc<dyn SoundfontBase>>),
    /// Sets the layer count for the soundfont
    SetLayerCount(Option<usize>),
}

#[derive(Debug, Clone)]
pub enum ChannelAudioEvent {
    /// Starts a new note vocice
    NoteOn { key: u8, vel: u8 },
    /// Signals off to a note voice
    NoteOff { key: u8 },
    /// Signal off to all voices
    AllNotesOff,
    /// Kill all voices without decay
    AllNotesKilled,
    /// Control event for the channel
    Control(ControlEvent),
}

#[derive(Debug, Clone)]
pub enum ChannelEvent {
    /// Audio
    Audio(ChannelAudioEvent),

    /// Config event for the channel
    Config(ChannelConfigEvent),
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
