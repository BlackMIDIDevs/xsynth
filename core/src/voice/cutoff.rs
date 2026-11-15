use std::{marker::PhantomData, sync::Arc};

use simdeez::prelude::*;
use xsynth_soundfonts::{
    modulation::{ModDestination, RegionModulation},
    FilterType,
};

use crate::{
    effects::{BiQuadFilter, SIMDBiQuadCoeffs, SIMDBiQuadState},
    helpers::db_to_amp,
    voice::{modulation_sum, ReleaseType, SIMDVoiceGenerator, VoiceControlData},
};

/// Controller-driven cutoff/resonance of a voice filter.
#[derive(Clone)]
pub(crate) struct CutoffModulation {
    modulation: Arc<RegionModulation>,
    filter_type: FilterType,
    cutoff: f32,
    resonance: f32,
    sample_rate: f32,
    last: (f32, f32),
}

impl CutoffModulation {
    pub fn new(
        modulation: Arc<RegionModulation>,
        filter_type: FilterType,
        cutoff: f32,
        resonance: f32,
        sample_rate: f32,
    ) -> Option<Self> {
        if !modulation.has_destination(|d| {
            matches!(d, ModDestination::CutoffCents | ModDestination::ResonanceDb)
        }) {
            return None;
        }
        Some(Self {
            modulation,
            filter_type,
            cutoff,
            resonance,
            sample_rate,
            last: (cutoff, resonance),
        })
    }

    /// Returns new (cutoff, Q) values if they changed.
    fn update(&mut self, control: &VoiceControlData) -> Option<(f32, f32)> {
        let c = &control.controllers;
        let m = &*self.modulation;
        let cents = modulation_sum(m, c, |d| *d == ModDestination::CutoffCents);
        let db = modulation_sum(m, c, |d| *d == ModDestination::ResonanceDb);
        let freq =
            (self.cutoff * 2.0f32.powf(cents / 1200.0)).clamp(1.0, self.sample_rate / 2.0 - 100.0);
        let q = self.resonance * db_to_amp(db);
        if (freq, q) != self.last {
            self.last = (freq, q);
            Some((freq, q))
        } else {
            None
        }
    }

    fn coeffs<S: Simd>(&self, freq: f32, q: f32) -> Arc<SIMDBiQuadCoeffs<S>> {
        Arc::new(SIMDBiQuadCoeffs::new(BiQuadFilter::get_coeffs(
            self.filter_type,
            freq,
            self.sample_rate,
            Some(q),
        )))
    }

    /// Returns new filter coefficients if the controllers changed them.
    fn updated_coeffs<S: Simd>(
        &mut self,
        control: &VoiceControlData,
    ) -> Option<Arc<SIMDBiQuadCoeffs<S>>> {
        self.update(control).map(|(freq, q)| self.coeffs(freq, q))
    }

    /// Returns the filter coefficients for the initial controller values.
    fn initial_coeffs<S: Simd>(
        &mut self,
        control: &VoiceControlData,
        base: &Arc<SIMDBiQuadCoeffs<S>>,
    ) -> Arc<SIMDBiQuadCoeffs<S>> {
        let initial = self.last;
        match self.update(control) {
            Some((freq, q)) if (freq, q) != initial => self.coeffs(freq, q),
            _ => base.clone(),
        }
    }
}

use super::{SIMDSampleMono, SIMDSampleStereo, VoiceGeneratorBase};

pub struct SIMDMonoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
{
    v: V,
    coeffs: Arc<SIMDBiQuadCoeffs<S>>,
    state: SIMDBiQuadState,
    modulation: Option<Box<CutoffModulation>>,
    _s: PhantomData<S>,
}

impl<S, V> SIMDMonoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
{
    pub(crate) fn new_modulated(
        v: V,
        coeffs: &Arc<SIMDBiQuadCoeffs<S>>,
        modulation: Option<CutoffModulation>,
        control: &VoiceControlData,
    ) -> Self {
        let mut modulation = modulation.map(Box::new);
        let coeffs = match modulation.as_mut() {
            Some(m) => m.initial_coeffs(control, coeffs),
            None => coeffs.clone(),
        };
        SIMDMonoVoiceCutoff {
            v,
            coeffs,
            state: Default::default(),
            modulation,
            _s: PhantomData,
        }
    }
}

impl<S, V> VoiceGeneratorBase for SIMDMonoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
{
    #[inline(always)]
    fn ended(&self) -> bool {
        self.v.ended()
    }

    #[inline(always)]
    fn signal_release(&mut self, rel_type: ReleaseType) {
        self.v.signal_release(rel_type);
    }

    #[inline(always)]
    fn process_controls(&mut self, control: &VoiceControlData) {
        self.v.process_controls(control);
        if let Some(m) = self.modulation.as_mut() {
            if let Some(coeffs) = m.updated_coeffs(control) {
                self.coeffs = coeffs;
            }
        }
    }

    #[inline(always)]
    fn prefetch(&self, frames: usize) {
        self.v.prefetch(frames)
    }
}

impl<S, V> SIMDVoiceGenerator<S, SIMDSampleMono<S>> for SIMDMonoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
{
    #[inline(always)]
    fn next_sample(&mut self) -> SIMDSampleMono<S> {
        let mut next_sample = self.v.next_sample();
        next_sample.0 = self.coeffs.process(&mut self.state, next_sample.0);
        next_sample
    }
}

pub struct SIMDStereoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
{
    v: V,
    coeffs: Arc<SIMDBiQuadCoeffs<S>>,
    state_left: SIMDBiQuadState,
    state_right: SIMDBiQuadState,
    modulation: Option<Box<CutoffModulation>>,
    _s: PhantomData<S>,
}

impl<S, V> SIMDStereoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
{
    pub(crate) fn new_modulated(
        v: V,
        coeffs: &Arc<SIMDBiQuadCoeffs<S>>,
        modulation: Option<CutoffModulation>,
        control: &VoiceControlData,
    ) -> Self {
        let mut modulation = modulation.map(Box::new);
        let coeffs = match modulation.as_mut() {
            Some(m) => m.initial_coeffs(control, coeffs),
            None => coeffs.clone(),
        };
        SIMDStereoVoiceCutoff {
            v,
            coeffs,
            state_left: Default::default(),
            state_right: Default::default(),
            modulation,
            _s: PhantomData,
        }
    }
}

impl<S, V> VoiceGeneratorBase for SIMDStereoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
{
    #[inline(always)]
    fn ended(&self) -> bool {
        self.v.ended()
    }

    #[inline(always)]
    fn signal_release(&mut self, rel_type: ReleaseType) {
        self.v.signal_release(rel_type);
    }

    #[inline(always)]
    fn process_controls(&mut self, control: &VoiceControlData) {
        self.v.process_controls(control);
        if let Some(m) = self.modulation.as_mut() {
            if let Some(coeffs) = m.updated_coeffs(control) {
                self.coeffs = coeffs;
            }
        }
    }

    #[inline(always)]
    fn prefetch(&self, frames: usize) {
        self.v.prefetch(frames)
    }
}

impl<S, V> SIMDVoiceGenerator<S, SIMDSampleStereo<S>> for SIMDStereoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
{
    #[inline(always)]
    fn next_sample(&mut self) -> SIMDSampleStereo<S> {
        let mut next_sample = self.v.next_sample();
        next_sample.0 = self.coeffs.process(&mut self.state_left, next_sample.0);
        next_sample.1 = self.coeffs.process(&mut self.state_right, next_sample.1);
        next_sample
    }
}
