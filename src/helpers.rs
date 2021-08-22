use std::ops::{Deref, DerefMut};

use simdeez::*; // nuts
use simdeez::avx2::*;
use simdeez::scalar::*;
use simdeez::sse2::*;
use simdeez::sse41::*;

use lazy_static::lazy_static;

pub struct Cache<T>(Option<T>);

pub struct CacheGuard<'a, T> {
    value: Option<T>,
    cache: &'a mut Cache<T>,
}

impl<T> Cache<T> {
    pub fn new(value: T) -> Cache<T> {
        Cache(Some(value))
    }

    pub fn get<'a>(&'a mut self) -> CacheGuard<'a, T> {
        match self.0.take() {
            None => panic!("Tried to fetch cache twice"),
            Some(v) => CacheGuard {
                value: Some(v),
                cache: self,
            },
        }
    }
}

impl<'a, T> Drop for CacheGuard<'a, T> {
    fn drop(&mut self) {
        self.cache.0.insert(self.value.take().unwrap());
    }
}

impl<'a, T> Deref for CacheGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value.as_ref().unwrap()
    }
}

impl<'a, T> DerefMut for CacheGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value.as_mut().unwrap()
    }
}

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

pub fn prepapre_cache_vec<T: Copy>(vec: &mut Vec<T>, len: usize, default: T) {
    if vec.len() < len {
        vec.reserve(len - vec.len());
    }
    unsafe {
        vec.set_len(len);
    }
    vec.fill(default);
}

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
    use crate::helpers::{Cache, sum_simd};

    #[test] 
    fn test_simd_add() {
        let mut src = vec![1.0, 2.0, 3.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        let mut dst = vec![0.0, 1.0, 3.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        sum_simd(&mut src, &mut dst);
        assert_eq!(dst, vec![1.0, 3.0, 6.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0]);
    }

    #[test]
    fn test_cache() {
        let mut cache = Cache::new(vec![1, 2, 3]);
        {
            let mut vec = cache.get();
            assert_eq!(vec[0], 1);
            vec[0] = 5;
            assert_eq!(vec[0], 5);
        }

        {
            let vec = cache.get();
            assert_eq!(vec[0], 5);
        }
    }
}
