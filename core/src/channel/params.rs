use std::sync::{atomic::AtomicU64, Arc};

use crate::AudioStreamParams;

use super::{channel_sf::ChannelSoundfont, ChannelConfigEvent};

/// Holds the statistics for an instance of VoiceChannel.
#[derive(Debug, Clone)]
pub struct VoiceChannelStats {
    pub(super) voice_counter: Arc<AtomicU64>,
}

/// Reads the statistics of an instance of VoiceChannel in a usable way.
pub struct VoiceChannelStatsReader {
    stats: VoiceChannelStats,
}

#[derive(Debug, Clone)]
pub struct VoiceChannelConst {
    pub stream_params: AudioStreamParams,
}

pub struct VoiceChannelParams {
    pub stats: VoiceChannelStats,
    pub layers: Option<usize>,
    pub channel_sf: ChannelSoundfont,
    pub constant: VoiceChannelConst,
}

impl VoiceChannelStats {
    pub fn new() -> Self {
        let voice_counter = Arc::new(AtomicU64::new(0));
        Self { voice_counter }
    }
}

impl Default for VoiceChannelStats {
    fn default() -> Self {
        Self::new()
    }
}

impl VoiceChannelParams {
    pub fn new(stream_params: AudioStreamParams) -> Self {
        let channel_sf = ChannelSoundfont::new();

        Self {
            stats: VoiceChannelStats::new(),
            layers: Some(4),
            channel_sf,
            constant: VoiceChannelConst { stream_params },
        }
    }

    pub fn process_config_event(&mut self, event: ChannelConfigEvent) {
        match event {
            ChannelConfigEvent::SetSoundfonts(soundfonts) => {
                self.channel_sf.set_soundfonts(soundfonts)
            }
            ChannelConfigEvent::SetLayerCount(count) => {
                self.layers = count;
            }
        }
    }
}

impl VoiceChannelStatsReader {
    pub(super) fn new(stats: VoiceChannelStats) -> Self {
        Self { stats }
    }

    /// The active voice count of the VoiceChannel.
    pub fn voice_count(&self) -> u64 {
        self.stats
            .voice_counter
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}
