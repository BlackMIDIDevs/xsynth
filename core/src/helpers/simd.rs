use simdeez::*; // nuts

use simdeez::avx2::*;
use simdeez::scalar::*;
use simdeez::sse2::*;
use simdeez::sse41::*;

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
    use super::sum_simd;

    #[test]
    fn test_simd_add() {
        let mut src = vec![1.0, 2.0, 3.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        let mut dst = vec![0.0, 1.0, 3.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        sum_simd(&mut src, &mut dst);
        assert_eq!(dst, vec![1.0, 3.0, 6.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0]);
    }
}
