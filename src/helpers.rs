use simdeez::*; // nuts

use simdeez::avx2::*;
use simdeez::scalar::*;
use simdeez::sse2::*;
use simdeez::sse41::*;

use lazy_static::lazy_static;

/// Create an array of key frequencies for keys 0-127
fn build_frequencies() -> [f32; 128] {
    let mut freqs = [0.0f32; 128];
    for key in 0..freqs.len() {
        freqs[key] = 2.0f32.powf((key as f32 - 69.0) / 12.0) * 440.0;
    }
    freqs
}

lazy_static! {
    pub static ref FREQS: [f32; 128] = build_frequencies();
}

/// Take any f32 vec, set its length and fill it with the default value
pub fn prepapre_cache_vec<T: Copy>(vec: &mut Vec<T>, len: usize, default: T) {
    if vec.len() < len {
        vec.reserve(len - vec.len());
    }
    unsafe {
        vec.set_len(len);
    }
    vec.fill(default);
}

/// Sum the values of `source` to the values of `target`, writing to `target`.
///
/// Uses runtime selected SIMD operations.
pub fn sum_simd(source: &[f32], target: &mut [f32]) {
    simd_runtime_generate!(
        // Altered code from the SIMD example here https://github.com/jackmott/simdeez
        fn sum(source: &[f32], target: &mut [f32]) {
            let mut source = &source[..source.len()];
            let mut target = &mut target[..source.len()];

            while source.len() >= S::VF32_WIDTH {
                let src = S::loadu_ps(&source[0]);
                let src2 = S::loadu_ps(&target[0]);

                S::storeu_ps(&mut target[0], src + src2);

                source = &source[S::VF32_WIDTH..];
                target = &mut target[S::VF32_WIDTH..];
            }

            for i in 0..source.len() {
                target[i] += source[i];
            }
        }
    );

    sum_runtime_select(source, target);
}

#[cfg(test)]
mod tests {
    use crate::helpers::sum_simd;

    #[test]
    fn test_simd_add() {
        let mut src = vec![1.0, 2.0, 3.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        let mut dst = vec![0.0, 1.0, 3.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        sum_simd(&mut src, &mut dst);
        assert_eq!(dst, vec![1.0, 3.0, 6.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0]);
    }
}
