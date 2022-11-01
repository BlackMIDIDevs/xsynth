use core::channel::{ChannelAudioEvent, ChannelConfigEvent};

pub enum SynthEvent {
    Channel(u32, ChannelAudioEvent),
    AllChannels(ChannelAudioEvent),
    ChannelConfig(ChannelConfigEvent),
}
