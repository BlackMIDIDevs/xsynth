use std::sync::Arc;

mod frequencies;
pub use frequencies::*;

mod simd;
pub use simd::*;

/// Take any f32 vec, set its length and fill it with the default value.
pub fn prepapre_cache_vec<T: Copy>(vec: &mut Vec<T>, len: usize, default: T) {
    if vec.len() < len {
        vec.reserve(len - vec.len());
    }
    unsafe {
        vec.set_len(len);
    }
    vec.fill(default);
}

/// Converts a dB value to 0-1 amplitude.
pub fn db_to_amp(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Checks if two `Arc<T>` vecs are equal based on `Arc::ptr_eq`.
pub fn are_arc_vecs_equal<T: ?Sized>(old: &[Arc<T>], new: &[Arc<T>]) -> bool {
    // First, check if the lengths are the same
    if old.len() != new.len() {
        return false;
    }

    // Then, check each pair of elements using Arc::ptr_eq
    for (old_item, new_item) in old.iter().zip(new.iter()) {
        if !Arc::ptr_eq(old_item, new_item) {
            return false;
        }
    }

    true
}
