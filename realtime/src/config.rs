pub struct XSynthRealtimeConfig {
    pub render_window_ms: f64,
    pub channel_count: u32,
    pub use_threadpool: bool,
}

impl Default for XSynthRealtimeConfig {
    fn default() -> Self {
        Self {
            render_window_ms: 20.0,
            channel_count: 16,
            use_threadpool: false,
        }
    }
}
