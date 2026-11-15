use crate::voice::{ReleaseType, VoiceControlData};

use std::sync::Arc;

use super::{Voice, VoiceFilter, VoiceGeneratorBase, VoiceSampleGenerator};

/// A struct that tracks the highest level voice functionality.
/// Cache line aligned: with many voices and short buffers, every voice state
/// is loaded from memory once per buffer, so it should span as few lines as
/// possible.
#[repr(align(64))]
pub struct VoiceBase<T: Send + Sync + VoiceSampleGenerator> {
    sample_generator: T,
    releasing: bool,
    killed: bool,
    velocity: u8,
    exclusive_class: Option<u8>,
    /// Shared by the voices of a spawner; a thin pointer keeps voices small
    filter: Option<Arc<VoiceFilter>>,
}

impl<T: Send + Sync + VoiceSampleGenerator> VoiceBase<T> {
    pub fn new(velocity: u8, exclusive_class: Option<u8>, sample_generator: T) -> VoiceBase<T> {
        VoiceBase {
            sample_generator,
            releasing: false,
            killed: false,
            velocity,
            exclusive_class,
            filter: None,
        }
    }

    /// Sets the filter the key applies to the output of this voice.
    pub(crate) fn with_filter(mut self, filter: Option<Arc<VoiceFilter>>) -> Self {
        self.filter = filter;
        self
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

    #[inline(always)]
    fn prefetch(&self, frames: usize) {
        self.sample_generator.prefetch(frames)
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

    #[inline(always)]
    fn render_planar(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.sample_generator.render_planar(left, right)
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

    #[inline(always)]
    fn exclusive_class(&self) -> Option<u8> {
        self.exclusive_class
    }

    #[inline(always)]
    #[allow(private_interfaces)]
    fn shared_filter(&self) -> Option<&Arc<VoiceFilter>> {
        self.filter.as_ref()
    }
}
