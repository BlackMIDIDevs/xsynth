use std::ops::RangeInclusive;
pub use xsynth_core::{
    channel::ChannelInitOptions,
    channel_group::{SynthFormat, ThreadCount},
};

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

    /// Defines the format that the synthesizer will use. See the `SynthFormat`
    /// documentation for more information.
    ///
    /// Default: `SynthFormat::Midi`
    pub format: SynthFormat,

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
            format: Default::default(),
            multithreading: ThreadCount::None,
            ignore_range: 0..=0,
        }
    }
}
