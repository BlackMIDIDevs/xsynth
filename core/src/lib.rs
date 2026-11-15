#![allow(clippy::let_and_return)]
#![allow(non_local_definitions)]

/// Runs a block of SIMD code for the engine `S`, like `simdeez::simd_invoke!`.
///
/// When the whole crate is compiled with AVX2 and FMA (for example with
/// `-C target-cpu=native`), the target-feature trampoline of `simd_invoke!`
/// adds nothing, but it stops LLVM from inlining the nested voice generators
/// into one loop. The block is expanded in place instead.
#[cfg(all(target_feature = "avx2", target_feature = "fma"))]
macro_rules! simd_block {
    ($g:ident, $body:expr $(,)?) => {{
        let _ = core::marker::PhantomData::<$g>;
        $body
    }};
}

#[cfg(not(all(target_feature = "avx2", target_feature = "fma")))]
macro_rules! simd_block {
    ($g:ident, $body:expr $(,)?) => {
        simdeez::simd_invoke!($g, $body)
    };
}

pub mod buffered_renderer;

pub mod channel;

pub mod voice;

mod audio_pipe;
pub use audio_pipe::*;

mod audio_stream;
pub use audio_stream::*;

pub mod soundfont;

pub mod effects;

pub mod helpers;

pub mod channel_group;
