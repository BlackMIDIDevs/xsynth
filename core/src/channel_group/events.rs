use crate::channel::{ChannelAudioEvent, ChannelConfigEvent};

/// Wrapper enum for various events to be sent to a MIDI synthesizer.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum SynthEvent {
    /// An audio event to be sent to the specified channel.
    /// See `ChannelAudioEvent` documentation for more information.
    Channel(u32, ChannelAudioEvent),

    /// An audio event to be sent to all available channels.
    /// See `ChannelAudioEvent` documentation for more information.
    AllChannels(ChannelAudioEvent),

    /// Configuration event for all channels.
    /// See `ChannelConfigEvent` documentation for more information.
    ChannelConfig(ChannelConfigEvent),
}
