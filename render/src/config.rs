use core::{channel::ChannelInitOptions, soundfont::SoundfontInitOptions};

/// Supported audio formats of XSynthRender
#[derive(PartialEq, Clone, Copy)]
pub enum XSynthRenderAudioFormat {
    Wav,
}

/// Options for initializing a new XSynthRender object.
#[derive(Clone)]
pub struct XSynthRenderConfig {
    /// Channel initialization options (same for all channels).
    pub channel_init_options: ChannelInitOptions,

    /// Soundfont initialization options (same for all soundfonts).
    pub sf_init_options: SoundfontInitOptions,

    /// Amount of VoiceChannel objects to be created.
    /// (Number of MIDI channels)
    pub channel_count: u32,

    /// A vector which specifies which of the created channels (indexes) will be used for drums.
    /// For example in a conventional 16 MIDI channel setup where channel 10 is used for
    /// drums, the vector would be set as \[9\] (counting from 0).
    pub drums_channels: Vec<u32>,

    /// Whether or not to use a threadpool to render voices.
    pub use_threadpool: bool,

    /// Whether or not to limit the output audio.
    pub use_limiter: bool,

    /// Audio output sample rate
    pub sample_rate: u32,

    /// Audio output audio channels
    pub audio_channels: u16,

    /// Audio output format
    pub audio_format: XSynthRenderAudioFormat,
}

impl Default for XSynthRenderConfig {
    fn default() -> Self {
        Self {
            channel_init_options: Default::default(),
            sf_init_options: Default::default(),
            channel_count: 16,
            drums_channels: vec![9],
            use_threadpool: true,
            use_limiter: true,
            sample_rate: 48000,
            audio_channels: 2,
            audio_format: XSynthRenderAudioFormat::Wav,
        }
    }
}
