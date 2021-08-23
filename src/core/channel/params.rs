use std::sync::{atomic::AtomicU64, Arc};

use crate::{core::soundfont::SineSoundfont, AudioStreamParams};

use super::channel_sf::ChannelSoundfont;

#[derive(Debug, Clone)]
pub struct VoiceChannelStats {
    pub(super) voice_counter: Arc<AtomicU64>,
}

pub struct VoiceChannelStatsReader {
    stats: VoiceChannelStats
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
        let mut channel_sf = ChannelSoundfont::new();
        channel_sf.set_soundfonts(vec![Arc::new(SineSoundfont::new(sample_rate, channels))]);

        Self {
            stats: VoiceChannelStats::new(),
            layers: 4,
            channel_sf,
            constant: VoiceChannelConst {
                stream_params: AudioStreamParams::new(sample_rate, channels),
            },
        }
    }
}

impl VoiceChannelStatsReader {
    pub fn new(stats: VoiceChannelStats) -> Self {
        Self { stats }
    }

    pub fn voice_count(&self) -> u64 {
        self.stats.voice_counter.load(std::sync::atomic::Ordering::Relaxed)
    }
}