/// Number of audio channels.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum ChannelCount {
    Mono,
    Stereo,
}

impl ChannelCount {
    pub fn count(&self) -> u16 {
        match self {
            ChannelCount::Mono => 1,
            ChannelCount::Stereo => 2,
        }
    }

    pub fn from_count(count: u16) -> Option<Self> {
        match count {
            1 => Some(ChannelCount::Mono),
            2 => Some(ChannelCount::Stereo),
            _ => None,
        }
    }
}

impl From<u16> for ChannelCount {
    fn from(count: u16) -> Self {
        ChannelCount::from_count(count)
            .expect("Unsupported channel count, only mono and stereo are supported")
    }
}

/// Parameters of the output audio.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct AudioStreamParams {
    pub sample_rate: u32,
    pub channels: ChannelCount,
}

impl AudioStreamParams {
    pub fn new(sample_rate: u32, channels: ChannelCount) -> Self {
        Self {
            sample_rate,
            channels,
        }
    }
}
