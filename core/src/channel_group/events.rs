use crate::channel::ChannelEvent;

/// Wrapper enum for various events to be sent to a MIDI synthesizer.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum SynthEvent {
    /// A channel event to be sent to the specified channel.
    /// See `ChannelEvent` documentation for more information.
    Channel(u32, ChannelEvent),

    /// A channel event to be sent to all available channels.
    /// See `ChannelAudioEvent` documentation for more information.
    AllChannels(ChannelEvent),
}
