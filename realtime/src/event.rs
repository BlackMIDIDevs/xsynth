use core::event::ChannelEvent;

pub enum SynthEvent {
    Channel(u32, ChannelEvent),
    AllChannels(ChannelEvent),
}
