pub struct XSynthRealtimeConfig {
    pub render_window_ms: f64,
    pub channel_count: u32,
}

impl Default for XSynthRealtimeConfig {
    fn default() -> Self {
        Self {
            render_window_ms: 10.0,
            channel_count: 16,
        }
    }
}
