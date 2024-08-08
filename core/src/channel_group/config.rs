use crate::{channel::ChannelInitOptions, AudioStreamParams};

/// Use multithreading for all actions inside the synthesizer (more info at `ParallelismOptions`)
/// with automatically determined thread counts.
pub const AUTO_MULTITHREADING: ParallelismOptions = ParallelismOptions {
    channel: ThreadCount::Auto,
    key: ThreadCount::Auto,
};

/// Defines the multithreading options for each task that supports it.
#[derive(Clone)]
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
#[derive(Clone)]
pub struct ParallelismOptions {
    /// Render the MIDI channels parallel in a threadpool with the specified
    /// thread count.
    pub channel: ThreadCount,

    /// Render the individisual keys of each channel parallel in a threadpool
    /// with the specified thread count.
    pub key: ThreadCount,
}

/// Options for initializing a new ChannelGroup.
#[derive(Clone)]
pub struct ChannelGroupConfig {
    /// Channel initialization options (same for all channels).
    /// See the `ChannelInitOptions` documentation for more information.
    pub channel_init_options: ChannelInitOptions,

    /// Amount of VoiceChannel objects to be created
    /// (Number of MIDI channels)
    /// The MIDI 1 spec uses 16 channels.
    pub channel_count: u32,

    /// A vector which specifies which of the created channels (indexes) will be used for drums.
    ///
    /// For example in a conventional 16 MIDI channel setup where channel 10 is used for
    /// drums, the vector would be set as vec!\[9\] (counting from 0).
    pub drums_channels: Vec<u32>,

    /// Parameters of the output audio.
    /// See the `AudioStreamParams` documentation for more information.
    pub audio_params: AudioStreamParams,

    /// Options about the `ChannelGroup` instance's parallelism. See the `ParallelismOptions`
    /// documentation for more information.
    pub parallelism: ParallelismOptions,
}
