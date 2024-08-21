use std::sync::Arc;

use crate::soundfont::SoundfontBase;

/// MIDI events for a single key in a channel.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
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

/// Events to modify parameters of a channel.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum ChannelConfigEvent {
    /// Sets the soundfonts for the channel
    SetSoundfonts(Vec<Arc<dyn SoundfontBase>>),

    /// Sets the layer count for the soundfont
    SetLayerCount(Option<usize>),

    /// Controls whether the channel will be standard or percussion.
    /// Setting to `true` will make the channel only use percussion patches.
    SetPercussionMode(bool),
}

/// MIDI events for a channel.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum ChannelAudioEvent {
    /// Starts a new note voice
    NoteOn { key: u8, vel: u8 },

    /// Signals off to a note voice
    NoteOff { key: u8 },

    /// Signal off to all voices
    AllNotesOff,

    /// Kill all voices without decay
    AllNotesKilled,

    /// Resets all CC to their default values
    ResetControl,

    /// Control event for the channel
    Control(ControlEvent),

    /// Program change event
    ProgramChange(u8),
}

/// Wrapper enum for various events for a channel.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum ChannelEvent {
    /// Audio event
    Audio(ChannelAudioEvent),

    /// Configuration event for the channel
    Config(ChannelConfigEvent),
}

/// MIDI control events for a channel.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum ControlEvent {
    /// A raw control change event
    Raw(u8, u8),

    /// The pitch bend strength, in tones
    PitchBendSensitivity(f32),

    /// The pitch bend value, between -1 and 1
    PitchBendValue(f32),

    /// The pitch bend, product of value * sensitivity
    PitchBend(f32),

    /// Fine tune value in cents
    FineTune(f32),

    /// Coarse tune value in semitones
    CoarseTune(f32),
}
