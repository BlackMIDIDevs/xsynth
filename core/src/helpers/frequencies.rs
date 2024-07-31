use lazy_static::lazy_static;

/// Create an array of key frequencies for keys 0-127
fn build_frequencies() -> [f32; 128] {
    let mut freqs = [0.0f32; 128];
    for (key, freq) in freqs.iter_mut().enumerate() {
        *freq = 2.0f32.powf((key as f32 - 69.0) / 12.0) * 440.0;
    }
    freqs
}

lazy_static! {
    /// Static array of all frequencies for keys 0-127.
    pub static ref FREQS: [f32; 128] = build_frequencies();
}
