use xsynth_core::{channel::ChannelInitOptions, soundfont::SoundfontInitOptions};

#[derive(PartialEq, Clone, Copy)]
pub enum XSynthRenderAudioFormat {
    Wav,
}

#[derive(Clone, Copy)]
pub struct XSynthRenderConfig {
    pub channel_init_options: ChannelInitOptions,
    pub sf_init_options: SoundfontInitOptions,
    pub channel_count: u32,
    pub use_threadpool: bool,
    pub use_limiter: bool,
    pub sample_rate: u32,
    pub audio_channels: u16,
    pub audio_format: XSynthRenderAudioFormat,
}

impl Default for XSynthRenderConfig {
    fn default() -> Self {
        Self {
            channel_init_options: Default::default(),
            sf_init_options: Default::default(),
            channel_count: 16,
            use_threadpool: true,
            use_limiter: true,
            sample_rate: 48000,
            audio_channels: 2,
            audio_format: XSynthRenderAudioFormat::Wav,
        }
    }
}
