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

/// Type of curve to be used in certain envelope stages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum EnvelopeCurveType {
    /// Apply a linear curve to the envelope stage.
    /// This option is supported by the attack, decay and release stages.
    Linear,

    /// Apply an exponential curve to the envelope stage.
    /// The decay and release stages will use a concave curve, while the
    /// attack stage will use a convex curve.
    Exponential,
}

/// Options for the curves of a specific envelope.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct EnvelopeOptions {
    /// Controls the type of curve of the attack envelope stage. See the
    /// documentation of the `EnvelopeCurveType` enum for available options.
    ///
    /// Default: `Convex`
    pub attack_curve: EnvelopeCurveType,

    /// Controls the type of curve of the decay envelope stage. See the
    /// documentation of the `EnvelopeCurveType` enum for available options.
    ///
    /// Default: `Linear`
    pub decay_curve: EnvelopeCurveType,

    /// Controls the type of curve of the release envelope stage. See the
    /// documentation of the `EnvelopeCurveType` enum for available options.
    ///
    /// Default: `Linear`
    pub release_curve: EnvelopeCurveType,
}

impl Default for EnvelopeOptions {
    fn default() -> Self {
        Self {
            attack_curve: EnvelopeCurveType::Exponential,
            decay_curve: EnvelopeCurveType::Linear,
            release_curve: EnvelopeCurveType::Linear,
        }
    }
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

    /// Configures the volume envelope curves in the dB scale. See the
    /// documentation for `EnvelopeOptions` for more information.
    pub vol_envelope_options: EnvelopeOptions,

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
            vol_envelope_options: Default::default(),
            use_effects: true,
            interpolator: Interpolator::Nearest,
        }
    }
}
