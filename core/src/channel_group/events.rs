use crate::channel::{ChannelAudioEvent, ChannelConfigEvent};

/// Wrapper enum for various events to be sent to a MIDI synthesizer.
pub enum SynthEvent {
    /// An audio event to be sent to the specified channel
    Channel(u32, ChannelAudioEvent),

    /// An audio event to be sent to all available channels
    AllChannels(ChannelAudioEvent),

    /// Configuration event for all channels
    ChannelConfig(ChannelConfigEvent),
}
