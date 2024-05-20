use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use std::sync::Arc;

pub fn resample_vecs(
    vecs: Vec<Vec<f32>>,
    sample_rate: f32,
    new_sample_rate: f32,
) -> Arc<[Arc<[f32]>]> {
    vecs.into_iter()
        .map(|samples| {
            let params = SincInterpolationParameters {
                sinc_len: 32,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Linear,
                oversampling_factor: 128,
                window: WindowFunction::BlackmanHarris2,
            };

            let len = samples.len();
            let mut resampler = SincFixedIn::<f32>::new(
                new_sample_rate as f64 / sample_rate as f64,
                2.0,
                params,
                len,
                1,
            )
            .unwrap();
            resampler.process(&[samples], None).unwrap()[0]
                .clone()
                .into()
        })
        .collect()
}
