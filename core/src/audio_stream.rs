#[derive(Debug, Clone)]
pub struct AudioStreamParams {
    pub sample_rate: u32,
    pub channels: u16,
}

impl AudioStreamParams {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
        }
    }
}
