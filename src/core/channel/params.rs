use std::sync::{atomic::AtomicU64, Arc};

use crate::AudioStreamParams;

use super::channel_sf::ChannelSoundfont;

#[derive(Debug, Clone)]
pub struct VoiceChannelStats {
    pub voice_counter: Arc<AtomicU64>,
}

#[derive(Debug, Clone)]
pub struct VoiceChannelConst {
    pub stream_params: AudioStreamParams,
}

pub struct VoiceChannelParams {
    pub stats: VoiceChannelStats,
    pub layers: i32,
    pub channel_sf: ChannelSoundfont,
    pub constant: VoiceChannelConst,
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
            channel_sf: ChannelSoundfont::new(),
            constant: VoiceChannelConst {
                stream_params: AudioStreamParams::new(sample_rate, channels),
            },
        }
    }
}
