use rubato::{FftFixedIn, Resampler};
use std::sync::Arc;

pub fn resample_vecs(
    vecs: Vec<Vec<f32>>,
    sample_rate: f32,
    new_sample_rate: f32,
) -> Arc<[Arc<[f32]>]> {
    vecs.into_iter()
        .map(|samples| {
            let len = samples.len();
            let mut resampler =
                FftFixedIn::<f32>::new(sample_rate as usize, new_sample_rate as usize, len, 32, 1)
                    .unwrap();
            resampler.process(&[samples], None).unwrap()[0]
                .clone()
                .into()
        })
        .collect()
}
