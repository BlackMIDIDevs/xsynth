use crate::{channel::ChannelInitOptions, AudioStreamParams};

/// Controls the channel format that will be used in the synthesizer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum SynthFormat {
    /// Standard MIDI format with 16 channels. Channel 10 will be used for percussion.
    #[default]
    MidiSingle,

    /// Creates a custom number of channels with the default settings.
    Custom { channels: u32 },
}

/// Defines the multithreading options for each task that supports it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum ThreadCount {
    /// No multithreading. Run everything on the same thread.
    None,

    /// Run with multithreading, with an automatically determined thread count.
    /// Please read
    /// [this](https://docs.rs/rayon-core/1.5.0/rayon_core/struct.ThreadPoolBuilder.html#method.num_threads)
    /// for more information about the thread count selection.
    Auto,

    /// Run with multithreading, with the specified thread count.
    Manual(usize),
}

/// Options regarding which parts of the ChannelGroup should be multithreaded.
///
/// Responsibilities of a channel: processing input events for the channel,
/// dispatching per-key rendering of audio, applying filters to the final channel's audio
///
/// Responsibilities of a key: Rendering per-voice audio for all the voices stored in a
/// key for a channel. This is generally the most compute intensive part of the synth.
///
/// Best practices:
/// - As there are often 16 channels in MIDI, per-key multithreading can balance out the
///     load more evenly between CPU cores.
/// - However, per-key multithreading adds some overhead, so if the synth is invoked to
///     render very small sample counts each time (e.g. sub 1 millisecond), not using per-key
///     multithreading becomes more efficient.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct ParallelismOptions {
    /// Render the MIDI channels parallel in a threadpool with the specified
    /// thread count.
    pub channel: ThreadCount,

    /// Render the individisual keys of each channel parallel in a threadpool
    /// with the specified thread count.
    pub key: ThreadCount,
}

impl ParallelismOptions {
    pub const AUTO_PER_KEY: Self = ParallelismOptions {
        channel: ThreadCount::Auto,
        key: ThreadCount::Auto,
    };

    pub const AUTO_PER_CHANNEL: Self = ParallelismOptions {
        channel: ThreadCount::Auto,
        key: ThreadCount::None,
    };
}

impl Default for ParallelismOptions {
    fn default() -> Self {
        Self::AUTO_PER_KEY
    }
}

/// Options for initializing a new ChannelGroup.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct ChannelGroupConfig {
    /// Channel initialization options (same for all channels).
    /// See the `ChannelInitOptions` documentation for more information.
    pub channel_init_options: ChannelInitOptions,

    /// Defines the format that the synthesizer will use. See the `SynthFormat`
    /// documentation for more information.
    pub format: SynthFormat,

    /// Parameters of the output audio.
    /// See the `AudioStreamParams` documentation for more information.
    pub audio_params: AudioStreamParams,

    /// Options about the `ChannelGroup` instance's parallelism. See the `ParallelismOptions`
    /// documentation for more information.
    pub parallelism: ParallelismOptions,
}
