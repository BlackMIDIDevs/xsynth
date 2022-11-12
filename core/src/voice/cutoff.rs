use std::{
    marker::PhantomData,
};

use simdeez::Simd;

use crate::voice::{VoiceControlData, SIMDVoiceGenerator, SIMDSampleMono};

use super::VoiceGeneratorBase;

/// SIMD voice generator combiner based on a passed in function
pub struct SIMDVoiceCutoff<T, TO, V, F>
where
T: Simd,
TO: SIMDSampleMono<T>,
V: SIMDVoiceGenerator<T, TO>,
F: Fn(TO, f32) -> TO,
{
    v: V,
    freq: f32,
    func: F,
    _t: PhantomData<T>,
    _to: PhantomData<TO>,
}

impl<T, TO, V, F> SIMDVoiceCutoff<T, TO, V, F>
where
T: Simd,
TO: SIMDSampleMono<T>,
V: SIMDVoiceGenerator<T, TO>,
F: Fn(TO, f32) -> TO,
{
    pub fn new(v: V, freq: f32, func: F) -> Self {
        SIMDVoiceCutoff {
            v,
            func,
            freq,
            _t: PhantomData,
            _to: PhantomData,
        }
    }
}

impl<T, TO, V, F> VoiceGeneratorBase for SIMDVoiceCutoff<T, TO, V, F>
where
T: Simd,
TO: SIMDSampleMono<T>,
V: SIMDVoiceGenerator<T, TO>,
F: Sync + Send + Fn(TO, f32) -> TO,
{
    fn ended(&self) -> bool {
        self.v.ended()
    }

    fn signal_release(&mut self) {
        self.v.signal_release();
    }

    fn process_controls(&mut self, control: &VoiceControlData) {
        self.v.process_controls(control);
    }
}

impl<T, TO, V, F> SIMDVoiceGenerator<T, TO> for SIMDVoiceCutoff<T, TO, V, F>
where
T: Simd,
TO: SIMDSampleMono<T>,
V: SIMDVoiceGenerator<T, TO>,
F: Sync + Send + Fn(TO, f32) -> TO,
{
    fn next_sample(&mut self) -> TO {
        (self.func)(self.v.next_sample(), self.freq)
    }
}

/// Parent struct for base SIMD voice combination functions
pub struct VoiceCutoffSIMD<T: Simd>(PhantomData<T>);

impl<T: Simd> VoiceCutoffSIMD<T> {
    pub fn cutoff<TO, V>(voice: V, freq: f32) -> impl SIMDVoiceGenerator<T, TO>
    where
    TO: SIMDSampleMono<T>,
    V: SIMDVoiceGenerator<T, TO>,
    {
        #[inline(always)]
        fn cutoff<T>(a: SIMDSampleMono<T>, freq: f32) -> SIMDSampleMono
        where
        T: Simd,
        {
            let rc = 1.0 / (freq * 2.0 * core::f32::consts::PI);
            let dt = 1.0 / 48000 as f32;
            let alpha = dt / (rc + dt);

            a[0] *= alpha;

            for i in 1..T::VF32_WIDTH {
                a[i] = a[i - 1] + alpha * (a[i] - a[i - 1]);
            }

            a
        }

        SIMDVoiceCutoff::new(voice, freq, cutoff)
    }
}
