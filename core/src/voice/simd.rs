use std::{
    marker::PhantomData,
    ops::{Add, Mul},
};

use simdeez::prelude::*;

use crate::voice::{ReleaseType, VoiceControlData};

use super::VoiceGeneratorBase;

/// The base SIMD voice sample trait, generally either mono or stereo
pub trait SIMDSample<T: Simd>: Sync + Send {
    fn zero() -> Self;
}

/// Mono SIMD voice sample
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct SIMDSampleMono<T: Simd>(pub T::Vf32);

impl<T: Simd> Mul<SIMDSampleMono<T>> for SIMDSampleMono<T> {
    type Output = Self;

    fn mul(self, rhs: SIMDSampleMono<T>) -> Self {
        simd_invoke!(T, Self(self.0 * rhs.0))
    }
}

impl<T: Simd> Mul<SIMDSampleStereo<T>> for SIMDSampleMono<T> {
    type Output = SIMDSampleStereo<T>;

    fn mul(self, rhs: SIMDSampleStereo<T>) -> Self::Output {
        simd_invoke!(T, SIMDSampleStereo(self.0 * rhs.0, self.0 * rhs.1))
    }
}

impl<T: Simd> Add<SIMDSampleMono<T>> for SIMDSampleMono<T> {
    type Output = Self;

    fn add(self, rhs: SIMDSampleMono<T>) -> Self {
        simd_invoke!(T, Self(self.0 + rhs.0))
    }
}

impl<T: Simd> Add<SIMDSampleStereo<T>> for SIMDSampleMono<T> {
    type Output = SIMDSampleStereo<T>;

    fn add(self, rhs: SIMDSampleStereo<T>) -> Self::Output {
        simd_invoke!(T, SIMDSampleStereo(self.0 + rhs.0, self.0 + rhs.1))
    }
}

impl<T: Simd> SIMDSample<T> for SIMDSampleMono<T> {
    fn zero() -> Self {
        simd_invoke!(T, SIMDSampleMono(T::Vf32::zeroes()))
    }
}

/// Stereo SIMD voice sample
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct SIMDSampleStereo<T: Simd>(pub T::Vf32, pub T::Vf32);

impl<T: Simd> Mul<SIMDSampleStereo<T>> for SIMDSampleStereo<T> {
    type Output = Self;

    fn mul(self, rhs: SIMDSampleStereo<T>) -> Self {
        simd_invoke!(T, Self(self.0 * rhs.0, self.1 * rhs.1))
    }
}

impl<T: Simd> Mul<SIMDSampleMono<T>> for SIMDSampleStereo<T> {
    type Output = SIMDSampleStereo<T>;

    fn mul(self, rhs: SIMDSampleMono<T>) -> Self::Output {
        simd_invoke!(T, SIMDSampleStereo(self.0 * rhs.0, self.1 * rhs.0))
    }
}

impl<T: Simd> Add<SIMDSampleStereo<T>> for SIMDSampleStereo<T> {
    type Output = Self;

    fn add(self, rhs: SIMDSampleStereo<T>) -> Self {
        simd_invoke!(T, Self(self.0 + rhs.0, self.1 + rhs.1))
    }
}

impl<T: Simd> Add<SIMDSampleMono<T>> for SIMDSampleStereo<T> {
    type Output = SIMDSampleStereo<T>;

    fn add(self, rhs: SIMDSampleMono<T>) -> Self::Output {
        simd_invoke!(T, SIMDSampleStereo(self.0 + rhs.0, self.1 + rhs.0))
    }
}

impl<T: Simd> SIMDSample<T> for SIMDSampleStereo<T> {
    fn zero() -> Self {
        simd_invoke!(T, {
            let val = T::Vf32::zeroes();
            SIMDSampleStereo(val, val)
        })
    }
}

/// The base SIMD voice generator trait
pub trait SIMDVoiceGenerator<T: Simd, TO: SIMDSample<T>>: VoiceGeneratorBase {
    fn next_sample(&mut self) -> TO;
}

/// SIMD voice generator combiner based on a passed in function
pub struct SIMDVoiceCombine<T, TI, TO, V1, V2, F>
where
    T: Simd,
    TI: SIMDSample<T>,
    TO: SIMDSample<T>,
    V1: SIMDVoiceGenerator<T, TI>,
    V2: SIMDVoiceGenerator<T, TO>,
    F: Fn(TI, TO) -> TO,
{
    v1: V1,
    v2: V2,
    func: F,
    _t: PhantomData<T>,
    _ti: PhantomData<TI>,
    _to: PhantomData<TO>,
}

impl<T, TI, TO, V1, V2, F> SIMDVoiceCombine<T, TI, TO, V1, V2, F>
where
    T: Simd,
    TI: SIMDSample<T>,
    TO: SIMDSample<T>,
    V1: SIMDVoiceGenerator<T, TI>,
    V2: SIMDVoiceGenerator<T, TO>,
    F: Fn(TI, TO) -> TO,
{
    pub fn new(v1: V1, v2: V2, func: F) -> Self {
        SIMDVoiceCombine {
            v1,
            v2,
            func,
            _t: PhantomData,
            _ti: PhantomData,
            _to: PhantomData,
        }
    }
}

impl<T, TI, TO, V1, V2, F> VoiceGeneratorBase for SIMDVoiceCombine<T, TI, TO, V1, V2, F>
where
    T: Simd,
    TI: SIMDSample<T>,
    TO: SIMDSample<T>,
    V1: SIMDVoiceGenerator<T, TI>,
    V2: SIMDVoiceGenerator<T, TO>,
    F: Sync + Send + Fn(TI, TO) -> TO,
{
    #[inline(always)]
    fn ended(&self) -> bool {
        self.v1.ended() || self.v2.ended()
    }

    #[inline(always)]
    fn signal_release(&mut self, rel_type: ReleaseType) {
        self.v1.signal_release(rel_type);
        self.v2.signal_release(rel_type);
    }

    #[inline(always)]
    fn process_controls(&mut self, control: &VoiceControlData) {
        self.v1.process_controls(control);
        self.v2.process_controls(control);
    }
}

impl<T, TI, TO, V1, V2, F> SIMDVoiceGenerator<T, TO> for SIMDVoiceCombine<T, TI, TO, V1, V2, F>
where
    T: Simd,
    TI: SIMDSample<T>,
    TO: SIMDSample<T>,
    V1: SIMDVoiceGenerator<T, TI>,
    V2: SIMDVoiceGenerator<T, TO>,
    F: Sync + Send + Fn(TI, TO) -> TO,
{
    #[inline(always)]
    fn next_sample(&mut self) -> TO {
        simd_invoke!(T, {
            (self.func)(self.v1.next_sample(), self.v2.next_sample())
        })
    }
}

/// Parent struct for base SIMD voice combination functions
pub struct VoiceCombineSIMD<T: Simd>(PhantomData<T>);

impl<T: Simd> VoiceCombineSIMD<T> {
    pub fn mult<TI, TO, V1, V2>(voice1: V1, voice2: V2) -> impl SIMDVoiceGenerator<T, TO>
    where
        TI: SIMDSample<T> + Mul<TO, Output = TO>,
        TO: SIMDSample<T>,
        V1: SIMDVoiceGenerator<T, TI>,
        V2: SIMDVoiceGenerator<T, TO>,
    {
        #[inline(always)]
        fn mult<T, TI, TO>(a: TI, b: TO) -> TO
        where
            T: Simd,
            TI: SIMDSample<T> + Mul<TO, Output = TO>,
            TO: SIMDSample<T>,
        {
            a * b
        }

        SIMDVoiceCombine::new(voice1, voice2, mult)
    }

    pub fn sum<TI, TO, V1, V2>(voice1: V1, voice2: V2) -> impl SIMDVoiceGenerator<T, TO>
    where
        TI: SIMDSample<T> + Add<TO, Output = TO>,
        TO: SIMDSample<T>,
        V1: SIMDVoiceGenerator<T, TI>,
        V2: SIMDVoiceGenerator<T, TO>,
    {
        #[inline(always)]
        fn add<T, TI, TO>(a: TI, b: TO) -> TO
        where
            T: Simd,
            TI: SIMDSample<T> + Add<TO, Output = TO>,
            TO: SIMDSample<T>,
        {
            a + b
        }

        SIMDVoiceCombine::new(voice1, voice2, add)
    }
}

#[cfg(test)]
mod tests {
    use VoiceControlData;

    use super::*;

    #[test]
    fn test_simd_voice_combine() {
        simd_runtime_generate!(
            fn run() {
                struct StereoVoiceGenSIMD;
                struct MonoVoiceGenSIMD;

                impl VoiceGeneratorBase for StereoVoiceGenSIMD {
                    fn ended(&self) -> bool {
                        false
                    }

                    fn signal_release(&mut self, _rel_type: ReleaseType) {}

                    fn process_controls(&mut self, _control: &VoiceControlData) {}
                }

                impl VoiceGeneratorBase for MonoVoiceGenSIMD {
                    fn ended(&self) -> bool {
                        false
                    }

                    fn signal_release(&mut self, _rel_type: ReleaseType) {}

                    fn process_controls(&mut self, _control: &VoiceControlData) {}
                }

                impl<S: Simd> SIMDVoiceGenerator<S, SIMDSampleStereo<S>> for StereoVoiceGenSIMD {
                    fn next_sample(&mut self) -> SIMDSampleStereo<S> {
                        simd_invoke!(S, {
                            let new = S::Vf32::set1(1.0);
                            SIMDSampleStereo(new, new)
                        })
                    }
                }

                impl<S: Simd> SIMDVoiceGenerator<S, SIMDSampleMono<S>> for MonoVoiceGenSIMD {
                    fn next_sample(&mut self) -> SIMDSampleMono<S> {
                        simd_invoke!(S, {
                            let new = S::Vf32::set1(2.0);
                            SIMDSampleMono(new)
                        })
                    }
                }

                let mut add = VoiceCombineSIMD::<S>::sum(MonoVoiceGenSIMD, StereoVoiceGenSIMD);
                let mut mul = VoiceCombineSIMD::<S>::mult(MonoVoiceGenSIMD, StereoVoiceGenSIMD);

                let sample = add.next_sample();

                for i in 0..S::Vf32::WIDTH {
                    assert_eq!(sample.0[i], 3.0);
                    assert_eq!(sample.1[i], 3.0);
                }

                let sample = mul.next_sample();

                for i in 0..S::Vf32::WIDTH {
                    assert_eq!(sample.0[i], 2.0);
                    assert_eq!(sample.1[i], 2.0);
                }
            }
        );

        run();
    }
}
