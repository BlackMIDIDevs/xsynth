use rubato::{
    audioadapter_buffers::direct::SequentialSliceOfVecs, Async, FixedAsync, Resampler,
    SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use std::sync::Arc;

/// Resample multiple audio sample vectors
pub fn resample_vecs(
    vecs: Vec<Vec<f32>>,
    sample_rate: f32,
    new_sample_rate: f32,
) -> Arc<[Arc<[f32]>]> {
    vecs.into_iter()
        .map(|samples| resample_vec(samples, sample_rate, new_sample_rate))
        .collect()
}

/// Resample a single audio sample vector
pub fn resample_vec(vec: Vec<f32>, sample_rate: f32, new_sample_rate: f32) -> Arc<[f32]> {
    let params = SincInterpolationParameters {
        sinc_len: 32,
        f_cutoff: Some(0.95),
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 128,
        window: WindowFunction::BlackmanHarris2,
    };

    let len = vec.len();
    let mut resampler = Async::<f32>::new_sinc(
        new_sample_rate as f64 / sample_rate as f64,
        2.0,
        &params,
        len.max(1),
        1,
        FixedAsync::Input,
    )
    .unwrap();
    let input = [vec];
    let adapter = SequentialSliceOfVecs::new(&input, 1, len).unwrap();
    resampler
        .process_all(&adapter, len, None)
        .unwrap()
        .take_data()
        .into()
}
