use crate::channel::ValueLerp;
use biquad::*;
use simdeez::prelude::*;
use soundfonts::FilterType;

#[derive(Clone)]
pub struct BiQuadFilter {
    filter: DirectForm1<f32>,
}

impl BiQuadFilter {
    pub fn new(fil_type: FilterType, freq: f32, sample_rate: f32) -> Self {
        let coeffs = Self::get_coeffs(fil_type, freq, sample_rate);

        Self {
            filter: DirectForm1::<f32>::new(coeffs),
        }
    }

    fn get_coeffs(fil_type: FilterType, freq: f32, sample_rate: f32) -> Coefficients<f32> {
        match fil_type {
            FilterType::LowPass => Coefficients::<f32>::from_params(
                Type::LowPass,
                sample_rate.hz(),
                freq.hz(),
                Q_BUTTERWORTH_F32,
            )
            .unwrap(),
            FilterType::LowPassPole => Coefficients::<f32>::from_params(
                Type::SinglePoleLowPass,
                sample_rate.hz(),
                freq.hz(),
                Q_BUTTERWORTH_F32,
            )
            .unwrap(),
            FilterType::HighPass => Coefficients::<f32>::from_params(
                Type::HighPass,
                sample_rate.hz(),
                freq.hz(),
                Q_BUTTERWORTH_F32,
            )
            .unwrap(),
            FilterType::BandPass => Coefficients::<f32>::from_params(
                Type::BandPass,
                sample_rate.hz(),
                freq.hz(),
                Q_BUTTERWORTH_F32,
            )
            .unwrap(),
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

pub struct MultiChannelBiQuad {
    channels: Vec<BiQuadFilter>,
    fil_type: FilterType,
    value: ValueLerp,
    sample_rate: f32,
}

impl MultiChannelBiQuad {
    pub fn new(channels: usize, fil_type: FilterType, freq: f32, sample_rate: f32) -> Self {
        Self {
            channels: (0..channels)
                .map(|_| BiQuadFilter::new(fil_type, freq, sample_rate))
                .collect(),
            fil_type,
            value: ValueLerp::new(freq, sample_rate as u32),
            sample_rate,
        }
    }

    pub fn set_filter_type(&mut self, fil_type: FilterType, freq: f32) {
        self.value.set_end(freq);
        self.fil_type = fil_type;
    }

    fn set_coefficients(&mut self, freq: f32) {
        let coeffs = BiQuadFilter::get_coeffs(self.fil_type, freq, self.sample_rate);
        for filter in self.channels.iter_mut() {
            filter.set_coefficients(coeffs);
        }
    }

    pub fn process(&mut self, sample: &mut [f32]) {
        let channel_count = self.channels.len();
        for (i, s) in sample.iter_mut().enumerate() {
            if i % channel_count == 0 {
                let v = self.value.get_next();
                self.set_coefficients(v);
            }
            *s = self.channels[i % channel_count].process(*s);
        }
    }
}
