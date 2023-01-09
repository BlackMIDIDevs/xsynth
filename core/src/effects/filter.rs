use biquad::*;
use simdeez::Simd;
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

    pub fn process_simd<S: Simd>(&mut self, input: S::Vf32) -> S::Vf32 {
        let mut out = input;
        for i in 0..S::VF32_WIDTH {
            out[i] = self.process(input[i]);
        }
        out
    }
}

pub struct MultiChannelBiQuad {
    channels: Vec<BiQuadFilter>,
    sample_rate: f32,
}

impl MultiChannelBiQuad {
    pub fn new(channels: usize, fil_type: FilterType, freq: f32, sample_rate: f32) -> Self {
        Self {
            channels: (0..channels)
                .map(|_| BiQuadFilter::new(fil_type, freq, sample_rate))
                .collect(),
            sample_rate,
        }
    }

    pub fn set_filter_type(&mut self, fil_type: FilterType, freq: f32) {
        let coeffs = BiQuadFilter::get_coeffs(fil_type, freq, self.sample_rate);
        for filter in self.channels.iter_mut() {
            filter.set_coefficients(coeffs);
        }
    }

    pub fn process(&mut self, sample: &mut [f32]) {
        let channel_count = self.channels.len();
        for (i, s) in sample.iter_mut().enumerate() {
            *s = self.channels[i % channel_count].process(*s);
        }
    }
}
