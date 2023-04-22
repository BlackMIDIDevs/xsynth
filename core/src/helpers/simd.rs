use simdeez::*; // nuts

use simdeez::prelude::*;

/// Sum the values of `source` to the values of `target`, writing to `target`.
///
/// Uses runtime selected SIMD operations.
pub fn sum_simd(source: &[f32], target: &mut [f32]) {
    simd_runtime_generate!(
        // Altered code from the SIMD example here https://github.com/jackmott/simdeez
        fn sum(source: &[f32], target: &mut [f32]) {
            let mut source = &source[..source.len()];
            let mut target = &mut target[..source.len()];

            loop {
                let src = S::Vf32::load_from_slice(source);
                let src2 = S::Vf32::load_from_slice(target);
                let sum = src + src2;

                sum.copy_to_slice(target);

                if source.len() <= S::Vf32::WIDTH {
                    break;
                }

                source = &source[S::Vf32::WIDTH..];
                target = &mut target[S::Vf32::WIDTH..];
            }
        }
    );

    sum(source, target);
}

#[cfg(test)]
mod tests {
    use super::sum_simd;

    #[test]
    fn test_simd_add() {
        let src = vec![1.0, 2.0, 3.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        let mut dst = vec![0.0, 1.0, 3.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        sum_simd(&src, &mut dst);
        assert_eq!(dst, vec![1.0, 3.0, 6.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0]);
    }
}
