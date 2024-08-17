/// Type of the audio sample interpolation algorithm.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum Interpolator {
    /// Nearest neighbor interpolation
    ///
    /// See more info about this method [here](https://en.wikipedia.org/wiki/Nearest-neighbor_interpolation)
    Nearest,

    /// Linear interpolation
    ///
    /// See more info about this method [here](https://en.wikipedia.org/wiki/Linear_interpolation)
    Linear,
}

/// Options for initializing/loading a new sample soundfont.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct SoundfontInitOptions {
    /// The bank number (0-128) to extract and use from the soundfont.
    /// `None` means to use all available banks (bank 0 for SFZ).
    ///
    /// Default: `None`
    pub bank: Option<u8>,

    /// The preset number (0-127) to extract and use from the soundfont.
    /// `None` means to use all available presets (preset 0 for SFZ).
    ///
    /// Default: `None`
    pub preset: Option<u8>,

    /// If set to true, the voices generated using this soundfont will
    /// release using a linear function instead of convex.
    ///
    /// Default: `false`
    pub linear_release: bool,

    /// If set to true, the voices generated using this soundfont will
    /// be able to use signal processing effects. Currently this option
    /// only affects the cutoff filter.
    ///
    /// Default: `true`
    pub use_effects: bool,

    /// The type of interpolator to use for the new soundfont. See the
    /// documentation of the `Interpolator` enum for available options.
    ///
    /// Default: `Nearest`
    pub interpolator: Interpolator,
}

impl Default for SoundfontInitOptions {
    fn default() -> Self {
        Self {
            bank: None,
            preset: None,
            linear_release: false,
            use_effects: true,
            interpolator: Interpolator::Nearest,
        }
    }
}
