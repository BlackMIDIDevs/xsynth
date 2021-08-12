use crate::core::event::ChannelEvent;

pub struct SynthEvent {
    pub channel: u32,
    pub event: ChannelEvent,
}

impl SynthEvent {
    pub fn new(channel: u32, event: ChannelEvent) -> Self {
        SynthEvent {
            channel: channel,
            event: event,
        }
    }
}
