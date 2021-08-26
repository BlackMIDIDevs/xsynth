#[derive(Debug)]
pub enum NoteEvent {
    On(u8),
    Off,
}

#[derive(Debug)]
pub enum ChannelEvent {
    NoteOn { key: u8, vel: u8 },
    NoteOff { key: u8 },
}
