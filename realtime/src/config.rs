use core::channel::ChannelInitOptions;

pub struct XSynthRealtimeConfig {
    pub channel_init_options: ChannelInitOptions,
    pub render_window_ms: f64,
    pub channel_count: u32,
    pub use_threadpool: bool,
}

impl Default for XSynthRealtimeConfig {
    fn default() -> Self {
        Self {
            channel_init_options: Default::default(),
            render_window_ms: 10.0,
            channel_count: 16,
            use_threadpool: false,
        }
    }
}
