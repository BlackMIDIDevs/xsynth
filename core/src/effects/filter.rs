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

    pub(crate) fn get_coeffs(
        fil_type: FilterType,
        freq: f32,
        sample_rate: f32,
        q: Option<f32>,
    ) -> Coefficients<f32> {
        let q = match q {
            Some(q) => q,
            None => Q_BUTTERWORTH_F32,
        };
        let freq = sanitize_freq(freq, sample_rate);

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
}

/// Coefficients of a biquad filter in block (state-space) form, for filtering
/// one SIMD vector of consecutive samples at a time.
///
/// The output of a block is a linear function of its inputs and of the filter
/// state before the block, so it is precomputed as one column per input sample
/// (the truncated impulse response) and one per state value. Filtering a vector
/// then costs `WIDTH + 4` independent fused multiply-adds instead of a serial
/// per-sample recursion.
pub(crate) struct SIMDBiQuadCoeffs<S: Simd> {
    /// The direct form coefficients, for samples that do not fill a vector
    coeffs: Coefficients<f32>,
    /// `inputs[j][n]`: contribution of input sample `j` to output sample `n`
    inputs: Box<[S::Vf32]>,
    x1: S::Vf32,
    x2: S::Vf32,
    y1: S::Vf32,
    y2: S::Vf32,
}

/// Per-channel state of a `SIMDBiQuadCoeffs` filter.
#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) struct SIMDBiQuadState {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl<S: Simd> SIMDBiQuadCoeffs<S> {
    pub fn new(coeffs: Coefficients<f32>) -> Self {
        let width = S::Vf32::WIDTH;
        let (b0, b1, b2) = (coeffs.b0 as f64, coeffs.b1 as f64, coeffs.b2 as f64);
        let (a1, a2) = (coeffs.a1 as f64, coeffs.a2 as f64);

        // Runs the direct form 1 recursion over one block from the given state.
        let run = |input: &[f64], mut x1: f64, mut x2: f64, mut y1: f64, mut y2: f64| {
            let mut out = vec![0.0f64; width];
            for n in 0..width {
                let x = input[n];
                let y = b0 * x + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
                x2 = x1;
                x1 = x;
                y2 = y1;
                y1 = y;
                out[n] = y;
            }
            out
        };
        let to_simd = |values: Vec<f64>| {
            simd_block!(S, {
                let mut v = S::Vf32::zeroes();
                for (i, value) in values.into_iter().enumerate() {
                    v[i] = value as f32;
                }
                v
            })
        };

        let zero = vec![0.0f64; width];
        let inputs = (0..width)
            .map(|j| {
                let mut impulse = zero.clone();
                impulse[j] = 1.0;
                to_simd(run(&impulse, 0.0, 0.0, 0.0, 0.0))
            })
            .collect();

        Self {
            coeffs,
            inputs,
            x1: to_simd(run(&zero, 1.0, 0.0, 0.0, 0.0)),
            x2: to_simd(run(&zero, 0.0, 1.0, 0.0, 0.0)),
            y1: to_simd(run(&zero, 0.0, 0.0, 1.0, 0.0)),
            y2: to_simd(run(&zero, 0.0, 0.0, 0.0, 1.0)),
        }
    }

    /// Filters one vector of consecutive samples.
    #[inline(always)]
    pub fn process(&self, state: &mut SIMDBiQuadState, input: S::Vf32) -> S::Vf32 {
        simd_block!(S, {
            let width = S::Vf32::WIDTH;
            // Safety: the input columns hold exactly WIDTH vectors.
            let inputs = unsafe { self.inputs.get_unchecked(..width) };

            // The input terms do not depend on the previous output, so they are
            // summed in two independent chains off the recursion's critical path.
            let mut acc_a = S::Vf32::set1(state.x1) * self.x1;
            let mut acc_b = S::Vf32::set1(state.x2) * self.x2;
            let mut j = 0;
            while j + 1 < width {
                acc_a = S::Vf32::set1(input[j]).mul_add(inputs[j], acc_a);
                acc_b = S::Vf32::set1(input[j + 1]).mul_add(inputs[j + 1], acc_b);
                j += 2;
            }
            if j < width {
                acc_a = S::Vf32::set1(input[j]).mul_add(inputs[j], acc_a);
            }
            let acc = S::Vf32::set1(state.y2).mul_add(self.y2, acc_a + acc_b);
            let out = S::Vf32::set1(state.y1).mul_add(self.y1, acc);

            if width >= 2 {
                state.x2 = input[width - 2];
                state.y2 = out[width - 2];
            } else {
                state.x2 = state.x1;
                state.y2 = state.y1;
            }
            state.x1 = input[width - 1];
            state.y1 = out[width - 1];
            out
        })
    }
}

fn sanitize_freq(freq: f32, sample_rate: f32) -> f32 {
    let nyquist = (sample_rate * 0.5).max(1.0);
    let max_freq = (nyquist - 1.0).max(1.0);
    freq.clamp(1.0, max_freq)
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
    ///   Butterworth value.
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

/// Identifies a set of filter coefficients: voices with equal ids can share
/// one filter.
pub(crate) type FilterId = [u32; 5];

/// A voice filter that can run on a bus of summed voices.
///
/// The filter is linear and time invariant, so filtering the sum of several
/// voices gives the sum of the voices filtered separately, with the bus state
/// being the sum of their states. Voices of a key that share coefficients are
/// therefore summed first and filtered once.
pub(crate) trait SharedFilter: Send + Sync {
    fn id(&self) -> FilterId;

    /// Filters a planar buffer in place: `frames` samples per channel, one
    /// state per channel.
    fn process_planar(&self, states: &mut [SIMDBiQuadState], buffer: &mut [f32], frames: usize);
}

impl<S: Simd> SIMDBiQuadCoeffs<S> {
    #[inline(always)]
    fn process_scalar(&self, state: &mut SIMDBiQuadState, x: f32) -> f32 {
        let c = &self.coeffs;
        let y = c.b0 * x + c.b1 * state.x1 + c.b2 * state.x2 - c.a1 * state.y1 - c.a2 * state.y2;
        state.x2 = state.x1;
        state.x1 = x;
        state.y2 = state.y1;
        state.y1 = y;
        y
    }
}

impl<S: Simd> SharedFilter for SIMDBiQuadCoeffs<S> {
    fn id(&self) -> FilterId {
        let c = &self.coeffs;
        [
            c.a1.to_bits(),
            c.a2.to_bits(),
            c.b0.to_bits(),
            c.b1.to_bits(),
            c.b2.to_bits(),
        ]
    }

    fn process_planar(&self, states: &mut [SIMDBiQuadState], buffer: &mut [f32], frames: usize) {
        simd_block!(S, {
            let width = S::Vf32::WIDTH;
            for (channel, state) in buffer.chunks_exact_mut(frames).zip(states.iter_mut()) {
                let mut chunks = channel.chunks_exact_mut(width);
                for chunk in &mut chunks {
                    let out = self.process(state, S::Vf32::load_from_slice(chunk));
                    out.copy_to_slice(chunk);
                }
                for x in chunks.into_remainder() {
                    *x = self.process_scalar(state, *x);
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use simdeez::simd_runtime_generate;

    #[test]
    fn block_biquad_matches_direct_form() {
        simd_runtime_generate!(
            fn run() {
                for (fil_type, freq, q) in [
                    (FilterType::LowPass, 1200.0, None),
                    (FilterType::LowPass, 19913.0, Some(0.9)),
                    (FilterType::HighPass, 300.0, Some(2.0)),
                    (FilterType::BandPass, 5000.0, None),
                    (FilterType::LowPassPole, 800.0, None),
                ] {
                    let coeffs = BiQuadFilter::get_coeffs(fil_type, freq, 48000.0, q);
                    // f64 reference of the same recursion
                    let c = coeffs;
                    let (mut x1, mut x2, mut y1, mut y2) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
                    let mut max_err = 0.0f64;
                    let mut scalar = BiQuadFilter::new(fil_type, freq, 48000.0, q);
                    let mut scalar_err = 0.0f64;
                    let block = SIMDBiQuadCoeffs::<S>::new(coeffs);
                    let mut state = SIMDBiQuadState::default();

                    let mut seed = 12345u32;
                    for _ in 0..200 {
                        let mut input = S::Vf32::zeroes();
                        for i in 0..S::Vf32::WIDTH {
                            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                            input[i] = (seed >> 8) as f32 / (1u32 << 23) as f32 - 1.0;
                        }
                        let out = block.process(&mut state, input);
                        for i in 0..S::Vf32::WIDTH {
                            let x = input[i] as f64;
                            let y = c.b0 as f64 * x + c.b1 as f64 * x1 + c.b2 as f64 * x2
                                - c.a1 as f64 * y1
                                - c.a2 as f64 * y2;
                            (x2, x1, y2, y1) = (x1, x, y1, y);
                            max_err = max_err.max((out[i] as f64 - y).abs());
                            let s = scalar.process(input[i]) as f64;
                            scalar_err = scalar_err.max((s - y).abs());
                        }
                    }
                    // The f32 block form rounds differently from the scalar recursion;
                    // high Q filters with low cutoffs drift the most (about -80 dB).
                    assert!(
                        max_err < 2e-4 && scalar_err < 2e-4,
                        "{fil_type:?} {freq}: error {max_err}, scalar {scalar_err}"
                    );
                }
            }
        );
        run();
    }
}
