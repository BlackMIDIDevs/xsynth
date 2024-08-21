use std::ops::RangeInclusive;
pub use xsynth_core::{channel::ChannelInitOptions, channel_group::ThreadCount};

/// Options for initializing a new RealtimeSynth.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct XSynthRealtimeConfig {
    /// Channel initialization options (same for all channels).
    /// See the `ChannelInitOptions` documentation for more information.
    pub channel_init_options: ChannelInitOptions,

    /// The length of the buffer reader in ms.
    ///
    /// Default: `10.0`
    pub render_window_ms: f64,

    /// Amount of VoiceChannel objects to be created (Number of MIDI channels).
    /// The MIDI 1 spec uses 16 channels. If the channel count is 16 or
    /// greater, then MIDI channel 10 will be set as the percussion channel.
    ///
    /// Default: `16`
    pub channel_count: u32,

    /// Controls the multithreading used for rendering per-voice audio for all
    /// the voices stored in a key for a channel. See the `ThreadCount` documentation
    /// for the available options.
    ///
    /// Default: `ThreadCount::None`
    pub multithreading: ThreadCount,

    /// A range of velocities that will not be played.
    ///
    /// Default: `0..=0`
    pub ignore_range: RangeInclusive<u8>,
}

impl Default for XSynthRealtimeConfig {
    fn default() -> Self {
        Self {
            channel_init_options: Default::default(),
            render_window_ms: 10.0,
            channel_count: 16,
            multithreading: ThreadCount::None,
            ignore_range: 0..=0,
        }
    }
}
