use std::sync::{Arc, RwLock};

use simdeez::Simd;

use crate::voice::VoiceControlData;

use super::{SIMDSampleMono, SIMDVoiceGenerator, VoiceGeneratorBase};

/// The stages in envelopes as a numbered enum
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum EnvelopeStage {
    Delay = 0,
    Attack = 1,
    Hold = 2,
    Decay = 3,
    Sustain = 4,
    Release = 5, // Goes to this stage as soon as the voice is released
    Finished = 6,
}

impl EnvelopeStage {
    pub fn as_usize(&self) -> usize {
        *self as usize
    }

    pub fn next_stage(&self) -> EnvelopeStage {
        match self {
            EnvelopeStage::Delay => EnvelopeStage::Attack,
            EnvelopeStage::Attack => EnvelopeStage::Hold,
            EnvelopeStage::Hold => EnvelopeStage::Decay,
            EnvelopeStage::Decay => EnvelopeStage::Sustain,
            EnvelopeStage::Sustain => EnvelopeStage::Release,
            EnvelopeStage::Release => EnvelopeStage::Finished,
            EnvelopeStage::Finished => EnvelopeStage::Finished,
        }
    }
}

// The lerp equation is `start + (end - start) * factor`
// We store: start, length (= end - start)
struct SIMDLerper<T: Simd> {
    start_simd: T::Vf32,
    length_simd: T::Vf32,
    start: f32,
    length: f32,
}

impl<T: Simd> SIMDLerper<T> {
    fn new(start: f32, end: f32) -> Self {
        unsafe {
            SIMDLerper {
                start_simd: T::set1_ps(start),
                length_simd: T::set1_ps(end - start),
                start,
                length: end - start,
            }
        }
    }

    fn lerp(&self, factor: f32) -> f32 {
        self.start + self.length * factor
    }

    fn lerp_simd(&self, factor: T::Vf32) -> T::Vf32 {
        self.start_simd + self.length_simd * factor
    }
}

struct StageTime<T: Simd> {
    stage_time_simd: T::Vf32,
    stage_end_time_f32: f32,
    increment_simd: T::Vf32,      // The SIMD width as a SIMD float
    stage_end_time_simd: T::Vf32, // The stage end time as a SIMD float
}

impl<T: Simd> StageTime<T> {
    fn new(start_offset: u32, stage_end_time: u32) -> Self {
        unsafe {
            let mut stage_time_simd = T::set1_ps(start_offset as f32);
            for i in 0..T::VF32_WIDTH {
                stage_time_simd[i] += i as f32;
            }

            StageTime {
                stage_time_simd,
                stage_end_time_f32: stage_end_time as f32,
                increment_simd: T::set1_ps(T::VF32_WIDTH as f32),
                stage_end_time_simd: T::set1_ps(stage_end_time as f32),
            }
        }
    }

    #[inline(always)]
    fn increment(&mut self) {
        self.stage_time_simd += self.increment_simd;
    }

    #[inline(always)]
    fn increment_by(&mut self, by: u32) {
        self.stage_time_simd += unsafe { T::set1_ps(by as f32) };
    }

    #[inline(always)]
    /// Is the upper most value in the SIMD array past the end?
    pub fn is_ending(&self) -> bool {
        self.simd_array_end_f32() >= self.stage_end_time_f32
    }

    #[inline(always)]
    /// Is the SIMD array intersecting the end? Or has it completely passed the end
    pub fn is_intersecting_end(&self) -> bool {
        self.is_ending() && self.simd_array_start_f32() < self.stage_end_time_f32
    }

    #[inline(always)]
    fn raw_simd_array(&self) -> &T::Vf32 {
        &self.stage_time_simd
    }

    #[inline(always)]
    pub fn progress_simd_array(&self) -> T::Vf32 {
        *self.raw_simd_array() / self.stage_end_time_simd
    }

    #[inline(always)]
    pub fn simd_array_start_f32(&self) -> f32 {
        self.stage_time_simd[0]
    }

    #[inline(always)]
    pub fn simd_array_end_f32(&self) -> f32 {
        self.stage_time_simd[T::VF32_WIDTH - 1]
    }

    #[allow(unused)]
    #[inline(always)]
    pub fn simd_array_start(&self) -> u32 {
        self.simd_array_start_f32() as u32
    }

    #[allow(unused)]
    #[inline(always)]
    pub fn simd_array_end(&self) -> u32 {
        self.simd_array_end_f32() as u32
    }
}

#[derive(Debug, Clone)]
pub enum EnvelopePart {
    Lerp {
        target: f32,   // Target value by the end of the envelope part
        duration: u32, // Duration in samples
    },
    Hold(f32),
}

impl EnvelopePart {
    pub fn lerp(target: f32, duration: u32) -> EnvelopePart {
        EnvelopePart::Lerp { target, duration }
    }

    pub fn hold(value: f32) -> EnvelopePart {
        EnvelopePart::Hold(value)
    }
}

/// The original envelope descriptor
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct EnvelopeDescriptor {
    pub start_percent: f32,   // % (0-1)
    pub delay: f32,           // Seconds
    pub attack: f32,          // Seconds
    pub hold: f32,            // Seconds
    pub decay: f32,           // Seconds
    pub sustain_percent: f32, // % (0-1)
    pub release: f32,         // Seconds
}

impl EnvelopeDescriptor {
    pub fn to_envelope_params(&self, samplerate: u32) -> EnvelopeParameters {
        let samplerate = samplerate as f32;

        EnvelopeParameters {
            start: self.start_percent,
            parts: [
                // Delay
                EnvelopePart::lerp(self.start_percent, (self.delay * samplerate) as u32),
                // Attack
                EnvelopePart::lerp(1.0, (self.attack * samplerate) as u32),
                // Hold
                EnvelopePart::lerp(1.0, (self.hold * samplerate) as u32),
                // Decay
                EnvelopePart::lerp(self.sustain_percent, (self.decay * samplerate) as u32),
                // Sustain
                EnvelopePart::hold(self.sustain_percent),
                // Release
                EnvelopePart::lerp(0.0, (self.release * samplerate) as u32),
                // Finished
                EnvelopePart::hold(0.0),
            ],
        }
    }
}

/// The raw envelope parameters used to generate the envelope.
/// Is a separate struct to EnvelopeDescriptor for performance reasons.
/// Use EnvelopeDescriptor to generate the EnvelopeParameters struct.
#[derive(Debug, Clone)]
pub struct EnvelopeParameters {
    start: f32,
    pub parts: [EnvelopePart; 7],
}

impl EnvelopeParameters {
    fn get_stage_data<T: Simd>(
        &self,
        stage: EnvelopeStage,
        start_amp: f32,
    ) -> VoiceEnvelopeState<T> {
        let stage_info = &self.parts[stage.as_usize()];
        match stage_info {
            EnvelopePart::Lerp { target, duration } => {
                let duration = *duration;
                let target = *target;
                if duration == 0 {
                    self.get_stage_data(stage.next_stage(), target)
                } else {
                    let data = StageData::Lerp(
                        SIMDLerper::new(start_amp, target),
                        StageTime::new(0, duration),
                    );
                    VoiceEnvelopeState {
                        current_stage: stage,
                        stage_data: data,
                    }
                }
            }
            EnvelopePart::Hold(value) => {
                let data = StageData::Constant(unsafe { T::set1_ps(*value) });
                VoiceEnvelopeState {
                    current_stage: stage,
                    stage_data: data,
                }
            }
        }
    }

    pub fn set_stage_data<T: Simd>(&mut self, part: usize, data: EnvelopePart) {
        self.parts[part] = data;
    }
}

enum StageData<T: Simd> {
    Lerp(SIMDLerper<T>, StageTime<T>),
    Constant(T::Vf32),
}

struct VoiceEnvelopeState<T: Simd> {
    current_stage: EnvelopeStage,
    stage_data: StageData<T>,
}

pub struct SIMDVoiceEnvelope<T: Simd> {
    params: Arc<RwLock<EnvelopeParameters>>,
    state: VoiceEnvelopeState<T>,
}

impl<T: Simd> SIMDVoiceEnvelope<T> {
    pub fn new(params: Arc<RwLock<EnvelopeParameters>>) -> Self {
        let state = params.read().unwrap().get_stage_data(EnvelopeStage::Delay, params.read().unwrap().start);

        SIMDVoiceEnvelope { params, state }
    }

    pub fn get_value_at_current_time(&self) -> f32 {
        match &self.state.stage_data {
            StageData::Lerp(lerper, stage_time) => {
                lerper.lerp(stage_time.simd_array_start_f32() / stage_time.stage_end_time_f32)
            }
            StageData::Constant(constant) => constant[0],
        }
    }

    pub fn current_stage(&self) -> &EnvelopeStage {
        &self.state.current_stage
    }

    fn switch_to_next_stage(&mut self) {
        let amp = self.get_value_at_current_time();
        self.state = self
            .params
            .read()
            .unwrap()
            .get_stage_data(self.current_stage().next_stage(), amp);
    }

    fn increment_time_by(&mut self, increment: u32) {
        match &mut self.state.stage_data {
            StageData::Lerp(_, stage_time) => {
                stage_time.increment_by(increment);
            }
            StageData::Constant(_) => {}
        }
    }

    fn manually_build_simd_sample(&mut self) -> SIMDSampleMono<T> {
        let mut values = unsafe { T::set1_ps(0.0) };
        for i in 0..T::VF32_WIDTH {
            let sample = self.get_value_at_current_time();
            values[i] = sample;
            self.increment_time_by(1);
            let should_progress = match &mut self.state.stage_data {
                StageData::Lerp(_, stage_time) => {
                    stage_time.is_ending() && !stage_time.is_intersecting_end()
                }
                StageData::Constant(_) => false,
            };
            if should_progress {
                self.switch_to_next_stage();
            }
        }
        SIMDSampleMono(values)
    }
}

impl<T: Simd> VoiceGeneratorBase for SIMDVoiceEnvelope<T> {
    fn ended(&self) -> bool {
        self.state.current_stage == EnvelopeStage::Finished
    }

    fn signal_release(&mut self) {
        let amp = self.get_value_at_current_time();
        self.state = self.params.read().unwrap().get_stage_data(EnvelopeStage::Release, amp);
    }

    fn process_controls(&mut self, _control: &VoiceControlData) {}
}

impl<T: Simd> SIMDVoiceGenerator<T, SIMDSampleMono<T>> for SIMDVoiceEnvelope<T> {
    fn next_sample(&mut self) -> SIMDSampleMono<T> {
        match &mut self.state.stage_data {
            StageData::Lerp(lerper, stage_time) => {
                if stage_time.is_ending() {
                    if stage_time.is_intersecting_end() {
                        // It is ended, and the SIMD array intersects the border of the envelope part.
                        // Therefore, this needs to generate one float sample at a time for this SIMD array.
                        self.manually_build_simd_sample()
                    } else {
                        // Is ended, except the SIMD array isn't intersecting the end.
                        // Therefore can jump to the next stage, and try again
                        self.switch_to_next_stage();
                        self.next_sample()
                    }
                } else {
                    // No special conditions happening, return the next entire simd array lerped
                    let values = lerper.lerp_simd(stage_time.progress_simd_array());
                    stage_time.increment();
                    SIMDSampleMono(values)
                }
            }
            StageData::Constant(constant) => SIMDSampleMono(*constant),
        }
    }
}

#[cfg(test)]
mod tests {
    use simdeez::simd_runtime_generate;
    use to_vec::ToVec;

    use super::*;

    use simdeez::*; // nuts

    use simdeez::avx2::*;
    use simdeez::scalar::*;
    use simdeez::sse2::*;
    use simdeez::sse41::*;

    fn assert_vf32_equal<S: Simd>(a: S::Vf32, b: S::Vf32) {
        for i in 0..S::VF32_WIDTH {
            assert_eq!(a[i], b[i]);
        }
    }

    fn simd_from_vec<S: Simd>(vec: Vec<f32>) -> S::Vf32 {
        unsafe {
            let mut initial = S::set1_ps(0.0);
            let mut iter = vec.into_iter();
            for i in 0..S::VF32_WIDTH {
                initial[i] = iter.next().unwrap();
            }
            initial
        }
    }

    #[test]
    fn test_simd_lerp() {
        simd_runtime_generate!(
            fn run() {
                let lerper = SIMDLerper::<S>::new(0.0, 1.0);
                assert_eq!(lerper.lerp(0.0), 0.0);
                assert_eq!(lerper.lerp(0.5), 0.5);
                assert_eq!(lerper.lerp(1.0), 1.0);
                assert_vf32_equal::<S>(lerper.lerp_simd(S::set1_ps(0.0)), S::set1_ps(0.0));
                assert_vf32_equal::<S>(lerper.lerp_simd(S::set1_ps(0.5)), S::set1_ps(0.5));
                assert_vf32_equal::<S>(lerper.lerp_simd(S::set1_ps(1.0)), S::set1_ps(1.0));
            }
        );

        run_runtime_select();
    }

    #[test]
    fn test_stage_time() {
        fn simd_from_range<S: Simd>(range: std::ops::Range<usize>) -> S::Vf32 {
            simd_from_vec::<S>(range.map(|v| v as f32).to_vec())
        }

        simd_runtime_generate!(
            fn run() {
                let mut time = StageTime::<S>::new(5, 20);
                let mut time2 = StageTime::<S>::new(4, 20);
                assert_eq!(time.simd_array_start(), 5);
                assert!(!time.is_ending());

                let end_simd = S::set1_ps(20.0);

                assert_vf32_equal::<S>(
                    *time.raw_simd_array(),
                    simd_from_range::<S>(5..(5 + S::VF32_WIDTH)),
                );
                assert_vf32_equal::<S>(
                    time.progress_simd_array(),
                    simd_from_range::<S>(5..(5 + S::VF32_WIDTH)) / end_simd,
                );

                let mut i = 5;
                while time.simd_array_start() + S::VF32_WIDTH as u32 <= 20 {
                    assert_vf32_equal::<S>(
                        *time.raw_simd_array(),
                        simd_from_range::<S>(i..(i + S::VF32_WIDTH)),
                    );
                    assert_vf32_equal::<S>(
                        time.progress_simd_array(),
                        simd_from_range::<S>(5..(5 + S::VF32_WIDTH)) / end_simd,
                    );
                    assert_eq!(time.simd_array_start(), i as u32);
                    assert!(!time.is_ending());

                    assert!(!time.is_intersecting_end());

                    time.increment();
                    time2.increment();
                    i += S::VF32_WIDTH;
                }
                assert_eq!(time.simd_array_start(), i as u32);
                assert!(time.is_ending());
                assert!(time.is_intersecting_end());

                assert!(!time2.is_ending());
                time2.increment();
                assert!(time2.is_ending());
                assert!(!time2.is_intersecting_end());
            }
        );

        run_runtime_select();
    }

    #[test]
    fn test_envelope() {
        #![allow(clippy::same_item_push)]

        fn push_simd_to_vec<S: Simd>(vec: &mut Vec<f32>, simd: S::Vf32) {
            for i in 0..S::VF32_WIDTH {
                vec.push(simd[i]);
            }
        }

        fn lerp(from: f32, to: f32, fac: f32) -> f32 {
            from + (to - from) * fac
        }

        simd_runtime_generate!(
            fn run() {
                let mut vec = Vec::new();

                let descriptor = EnvelopeDescriptor {
                    start_percent: 0.5,
                    delay: 0.0,
                    attack: 15.0,
                    hold: 0.0,
                    decay: 17.0,
                    sustain_percent: 0.4,
                    release: 16.0,
                };
                let params = Arc::new(descriptor.to_envelope_params(1));

                let mut env = SIMDVoiceEnvelope::<S>::new(params);

                let mut i = 0;
                while i < 48 {
                    push_simd_to_vec::<S>(&mut vec, env.next_sample().0);
                    i += S::VF32_WIDTH;
                }
                env.signal_release();
                assert_eq!(env.current_stage(), &EnvelopeStage::Release);
                while i < 48 + 32 {
                    push_simd_to_vec::<S>(&mut vec, env.next_sample().0);
                    i += S::VF32_WIDTH;
                }

                let mut expected_vec = Vec::new();

                for i in 0..15 {
                    expected_vec.push(lerp(0.5, 1.0, i as f32 / 15.0));
                }
                for i in 0..17 {
                    expected_vec.push(lerp(1.0, 0.4, i as f32 / 17.0));
                }
                for _ in 0..16 {
                    expected_vec.push(0.4);
                }
                for i in 0..16 {
                    expected_vec.push(lerp(0.4, 0.0, i as f32 / 16.0));
                }
                for _ in 0..16 {
                    expected_vec.push(0.0);
                }

                for v in vec.iter_mut().chain(expected_vec.iter_mut()) {
                    // Rounding as cached values are sometimes off by tiny fractions
                    *v = (*v * 10000.0).round() / 10000.0;
                }

                assert_eq!(vec, expected_vec);
            }
        );

        run_runtime_select();
    }
}
