pub mod resample;
pub mod sf2;
pub mod sfz;

/// Type of the audio filter used.
#[derive(Debug, PartialEq, Clone, Copy, Default)]
pub enum FilterType {
    /// First order low pass filter
    LowPassPole,

    /// Second order low pass filter
    #[default]
    LowPass,

    /// Second order high pass filter
    HighPass,

    /// Second order band pass filter
    BandPass,
}

/// Type of looping for a sample.
#[derive(Debug, PartialEq, Clone, Copy, Default)]
pub enum LoopMode {
    /// Do not loop the sample
    #[default]
    NoLoop,

    /// Play once from start to finish ignoring note off and envelope
    OneShot,

    /// Play from start and loop the specified region continuously
    LoopContinuous,

    /// Play from start, loop the specified region continuously and
    /// play the rest of the sample when released
    LoopSustain,
}

/// Converts the sample index of an audio sample array when
/// it is resampled.
pub fn convert_sample_index(idx: u32, old_sample_rate: u32, new_sample_rate: u32) -> u32 {
    (new_sample_rate as f32 * idx as f32 / old_sample_rate as f32).round() as u32
}
