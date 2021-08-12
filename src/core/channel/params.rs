use std::sync::{atomic::AtomicU64, Arc};

#[derive(Debug, Clone)]
pub struct VoiceChannelStats {
    pub voice_counter: Arc<AtomicU64>,
}

#[derive(Debug, Clone)]
pub struct VoiceChannelParams {
    pub stats: VoiceChannelStats,
    pub layers: i32,
    pub sample_rate: u32,
    pub channels: u16,
}

impl VoiceChannelStats {
    pub fn new() -> Self {
        let voice_counter = Arc::new(AtomicU64::new(0));
        Self { voice_counter }
    }
}

impl VoiceChannelParams {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            stats: VoiceChannelStats::new(),
            layers: 4,
            sample_rate,
            channels,
        }
    }
}
