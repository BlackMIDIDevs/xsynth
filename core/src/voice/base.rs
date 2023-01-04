use crate::voice::{ReleaseType, VoiceControlData};

use super::{Voice, VoiceGeneratorBase, VoiceSampleGenerator};

/// A struct that tracks the highest level voice functionality.
pub struct VoiceBase<T: Send + Sync + VoiceSampleGenerator> {
    sample_generator: T,
    releasing: bool,
    killed: bool,
    velocity: u8,
}

impl<T: Send + Sync + VoiceSampleGenerator> VoiceBase<T> {
    pub fn new(velocity: u8, sample_generator: T) -> VoiceBase<T> {
        VoiceBase {
            sample_generator,
            releasing: false,
            killed: false,
            velocity,
        }
    }
}

impl<T> VoiceGeneratorBase for VoiceBase<T>
where
    T: Send + Sync + VoiceSampleGenerator,
{
    #[inline(always)]
    fn ended(&self) -> bool {
        self.sample_generator.ended()
    }

    #[inline(always)]
    fn signal_release(&mut self, rel_type: ReleaseType) {
        match rel_type {
            ReleaseType::Standard => self.releasing = true,
            ReleaseType::Kill => self.killed = true,
        }
        self.sample_generator.signal_release(rel_type)
    }

    #[inline(always)]
    fn process_controls(&mut self, control: &VoiceControlData) {
        self.sample_generator.process_controls(control)
    }
}

impl<T> VoiceSampleGenerator for VoiceBase<T>
where
    T: Send + Sync + VoiceSampleGenerator,
{
    #[inline(always)]
    fn render_to(&mut self, buffer: &mut [f32]) {
        self.sample_generator.render_to(buffer)
    }
}

impl<T> Voice for VoiceBase<T>
where
    T: Send + Sync + VoiceSampleGenerator,
{
    #[inline(always)]
    fn is_releasing(&self) -> bool {
        self.releasing
    }

    #[inline(always)]
    fn is_killed(&self) -> bool {
        self.killed
    }

    #[inline(always)]
    fn velocity(&self) -> u8 {
        self.velocity
    }
}
