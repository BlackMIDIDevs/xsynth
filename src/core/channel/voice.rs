mod envelopes;
pub use envelopes::*;

mod simd;
pub use simd::*;

mod simdvoice;
pub use simdvoice::*;

mod base;
pub use base::*;

mod squarewave;
pub use squarewave::*;

mod channels;
pub use channels::*;

mod constant;
pub use constant::*;

pub trait VoiceGeneratorBase: Sync + Send {
    fn ended(&self) -> bool;
    fn signal_release(&mut self);
}

pub trait VoiceSampleGenerator: VoiceGeneratorBase {
    fn render_to(&mut self, buffer: &mut [f32]);
}

pub trait Voice: VoiceSampleGenerator + Send + Sync {
    fn is_releasing(&self) -> bool;
}
