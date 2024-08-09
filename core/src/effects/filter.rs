use crate::channel::ValueLerp;
use biquad::*;
use simdeez::prelude::*;
pub use xsynth_soundfonts::FilterType;

#[derive(Clone)]
pub(crate) struct BiQuadFilter {
    filter: DirectForm1<f32>,
}

impl BiQuadFilter {
    pub fn new(fil_type: FilterType, freq: f32, sample_rate: f32, q: Option<f32>) -> Self {
        let coeffs = Self::get_coeffs(fil_type, freq, sample_rate, q);

        Self {
            filter: DirectForm1::<f32>::new(coeffs),
        }
    }

    fn get_coeffs(
        fil_type: FilterType,
        freq: f32,
        sample_rate: f32,
        q: Option<f32>,
    ) -> Coefficients<f32> {
        let q = match q {
            Some(q) => q,
            None => Q_BUTTERWORTH_F32,
        };

        match fil_type {
            FilterType::LowPass => {
                Coefficients::<f32>::from_params(Type::LowPass, sample_rate.hz(), freq.hz(), q)
                    .unwrap()
            }
            FilterType::LowPassPole => Coefficients::<f32>::from_params(
                Type::SinglePoleLowPass,
                sample_rate.hz(),
                freq.hz(),
                q,
            )
            .unwrap(),
            FilterType::HighPass => {
                Coefficients::<f32>::from_params(Type::HighPass, sample_rate.hz(), freq.hz(), q)
                    .unwrap()
            }
            FilterType::BandPass => {
                Coefficients::<f32>::from_params(Type::BandPass, sample_rate.hz(), freq.hz(), q)
                    .unwrap()
            }
        }
    }

    pub fn set_coefficients(&mut self, coeffs: Coefficients<f32>) {
        self.filter.replace_coefficients(coeffs);
    }

    pub fn process(&mut self, input: f32) -> f32 {
        self.filter.run(input)
    }

    #[inline(always)]
    pub fn process_simd<S: Simd>(&mut self, input: S::Vf32) -> S::Vf32 {
        let mut out = input;
        for i in 0..S::Vf32::WIDTH {
            out[i] = self.process(input[i]);
        }
        out
    }
}

/// A multi-channel bi-quad audio filter.
///
/// Supports single pole low pass filter and two pole low pass, high pass
/// and band pass filters. For more information please see the `FilterType`
/// documentation.
///
/// Uses the `biquad` crate for signal processing.
pub struct MultiChannelBiQuad {
    channels: Vec<BiQuadFilter>,
    fil_type: FilterType,
    value: ValueLerp,
    q: Option<f32>,
    sample_rate: f32,
}

impl MultiChannelBiQuad {
    /// Creates a new audio filter with the given parameters.
    ///
    /// - `channels`: Number of audio channels
    /// - `fil_type`: Type of the audio filter. See FilterType docs
    /// - `freq`: Cutoff frequency
    /// - `sample_rate`: Sample rate of the audio to be processed
    /// - `q`: The Q parameter of the cutoff filter. Use None for the default
    ///         Butterworth value.
    pub fn new(
        channels: usize,
        fil_type: FilterType,
        freq: f32,
        sample_rate: f32,
        q: Option<f32>,
    ) -> Self {
        Self {
            channels: (0..channels)
                .map(|_| BiQuadFilter::new(fil_type, freq, sample_rate, q))
                .collect(),
            fil_type,
            value: ValueLerp::new(freq, sample_rate as u32),
            q,
            sample_rate,
        }
    }

    /// Changes the type of the audio filter.
    pub fn set_filter_type(&mut self, fil_type: FilterType, freq: f32, q: Option<f32>) {
        self.value.set_end(freq);
        self.fil_type = fil_type;
        self.q = q;
    }

    fn set_coefficients(&mut self, freq: f32, q: Option<f32>) {
        let coeffs = BiQuadFilter::get_coeffs(self.fil_type, freq, self.sample_rate, q);
        for filter in self.channels.iter_mut() {
            filter.set_coefficients(coeffs);
        }
    }

    /// Filters the audio of the given sample buffer.
    pub fn process(&mut self, sample: &mut [f32]) {
        let channel_count = self.channels.len();
        for (i, s) in sample.iter_mut().enumerate() {
            if i % channel_count == 0 {
                let v = self.value.get_next();
                self.set_coefficients(v, self.q);
            }
            *s = self.channels[i % channel_count].process(*s);
        }
    }
}
