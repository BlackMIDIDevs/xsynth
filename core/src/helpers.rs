mod frequencies;
pub use frequencies::*;

mod simd;
pub use simd::*;

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
