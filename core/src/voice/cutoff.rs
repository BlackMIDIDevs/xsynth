use std::{marker::PhantomData, sync::Arc};

use simdeez::prelude::*;
use xsynth_soundfonts::{
    modulation::{ModDestination, RegionModulation},
    FilterType,
};

use crate::{
    effects::BiQuadFilter,
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

    /// Applies the current controller values to a filter.
    pub fn apply_initial(&mut self, control: &VoiceControlData, filter: &mut BiQuadFilter) {
        self.last = (f32::NAN, f32::NAN);
        if let Some((freq, q)) = self.update(control) {
            filter.set_params(self.filter_type, freq, self.sample_rate, Some(q));
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
    cutoff: BiQuadFilter,
    modulation: Option<CutoffModulation>,
    _s: PhantomData<S>,
}

impl<S, V> SIMDMonoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
{
    pub fn new(v: V, filter: &BiQuadFilter) -> Self {
        SIMDMonoVoiceCutoff {
            v,
            cutoff: filter.clone(),
            modulation: None,
            _s: PhantomData,
        }
    }

    pub(crate) fn new_modulated(
        v: V,
        filter: &BiQuadFilter,
        modulation: Option<CutoffModulation>,
        control: &VoiceControlData,
    ) -> Self {
        let mut cutoff = filter.clone();
        let mut modulation = modulation;
        if let Some(m) = modulation.as_mut() {
            m.apply_initial(control, &mut cutoff);
        }
        SIMDMonoVoiceCutoff {
            v,
            cutoff,
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
            if let Some((freq, q)) = m.update(control) {
                self.cutoff
                    .set_params(m.filter_type, freq, m.sample_rate, Some(q));
            }
        }
    }
}

impl<S, V> SIMDVoiceGenerator<S, SIMDSampleMono<S>> for SIMDMonoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleMono<S>>,
{
    #[inline(always)]
    fn next_sample(&mut self) -> SIMDSampleMono<S> {
        simd_invoke!(S, {
            let mut next_sample = self.v.next_sample();
            next_sample.0 = self.cutoff.process_simd::<S>(next_sample.0);
            next_sample
        })
    }
}

pub struct SIMDStereoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
{
    v: V,
    cutoff1: BiQuadFilter,
    cutoff2: BiQuadFilter,
    modulation: Option<CutoffModulation>,
    _s: PhantomData<S>,
}

impl<S, V> SIMDStereoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
{
    pub fn new(v: V, filter: &BiQuadFilter) -> Self {
        SIMDStereoVoiceCutoff {
            v,
            cutoff1: filter.clone(),
            cutoff2: filter.clone(),
            modulation: None,
            _s: PhantomData,
        }
    }

    pub(crate) fn new_modulated(
        v: V,
        filter: &BiQuadFilter,
        modulation: Option<CutoffModulation>,
        control: &VoiceControlData,
    ) -> Self {
        let mut cutoff = filter.clone();
        let mut modulation = modulation;
        if let Some(m) = modulation.as_mut() {
            m.apply_initial(control, &mut cutoff);
        }
        SIMDStereoVoiceCutoff {
            v,
            cutoff1: cutoff.clone(),
            cutoff2: cutoff,
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
            if let Some((freq, q)) = m.update(control) {
                self.cutoff1
                    .set_params(m.filter_type, freq, m.sample_rate, Some(q));
                self.cutoff2
                    .set_params(m.filter_type, freq, m.sample_rate, Some(q));
            }
        }
    }
}

impl<S, V> SIMDVoiceGenerator<S, SIMDSampleStereo<S>> for SIMDStereoVoiceCutoff<S, V>
where
    S: Simd,
    V: SIMDVoiceGenerator<S, SIMDSampleStereo<S>>,
{
    #[inline(always)]
    fn next_sample(&mut self) -> SIMDSampleStereo<S> {
        simd_invoke!(S, {
            let mut next_sample = self.v.next_sample();
            next_sample.0 = self.cutoff1.process_simd::<S>(next_sample.0);
            next_sample.1 = self.cutoff2.process_simd::<S>(next_sample.1);
            next_sample
        })
    }
}
