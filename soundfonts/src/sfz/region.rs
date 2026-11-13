use std::{
    collections::VecDeque,
    ops::RangeInclusive,
    path::{Path, PathBuf},
};

use crate::{
    modulation::{
        CcCrossfade, CcRange, CrossfadeCurve, ModController, ModDestination, ModSource,
        RegionModulation, RuntimeModulator,
    },
    FilterType, LoopMode,
};

use super::parse::{SfzAmpegEnvelope, SfzCcTarget, SfzGroupType, SfzOpcode, SfzToken};

/// Structure that holds the opcode parameters of the SFZ's AmpEG envelope.
#[derive(Debug, Clone)]
pub struct AmpegEnvelopeParams {
    pub ampeg_start: f32,
    pub ampeg_delay: f32,
    pub ampeg_attack: f32,
    pub ampeg_hold: f32,
    pub ampeg_decay: f32,
    pub ampeg_sustain: f32,
    pub ampeg_release: f32,
    pub ampeg_vel2release: f32,
}

impl Default for AmpegEnvelopeParams {
    fn default() -> Self {
        AmpegEnvelopeParams {
            ampeg_start: 0.0,
            ampeg_delay: 0.0,
            ampeg_attack: 0.01,
            ampeg_hold: 0.0,
            ampeg_decay: 0.0,
            ampeg_sustain: 100.0,
            ampeg_release: 0.01,
            ampeg_vel2release: 0.0,
        }
    }
}

impl AmpegEnvelopeParams {
    fn update_from_flag(&mut self, flag: SfzAmpegEnvelope) {
        match flag {
            SfzAmpegEnvelope::AmpegStart(val) => self.ampeg_start = val,
            SfzAmpegEnvelope::AmpegDelay(val) => self.ampeg_delay = val,
            SfzAmpegEnvelope::AmpegAttack(val) => self.ampeg_attack = val,
            SfzAmpegEnvelope::AmpegHold(val) => self.ampeg_hold = val,
            SfzAmpegEnvelope::AmpegDecay(val) => self.ampeg_decay = val,
            SfzAmpegEnvelope::AmpegSustain(val) => self.ampeg_sustain = val,
            SfzAmpegEnvelope::AmpegRelease(val) => self.ampeg_release = val,
            SfzAmpegEnvelope::AmpegVel2Release(val) => self.ampeg_vel2release = val,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RegionParamsBuilder {
    lovel: u8,
    hivel: u8,
    lokey: i8,
    hikey: i8,
    pitch_keycenter: i8,
    volume: i16,
    pan: i8,
    sample: Option<String>,
    default_path: Option<String>,
    loop_mode: LoopMode,
    loop_start: u32,
    loop_end: u32,
    offset: u32,
    cutoff: Option<f32>,
    resonance: f32,
    amp_veltrack: f32,
    amp_keycenter: i8,
    amp_keytrack: f32,
    pan_veltrack: f32,
    pan_keycenter: i8,
    pan_keytrack: f32,
    fil_veltrack: i16,
    fil_keycenter: i8,
    fil_keytrack: i16,
    filter_type: FilterType,
    ampeg_envelope: AmpegEnvelopeParams,
    tune: i16,
    amplitude: f32,
    cc_mods: Vec<(SfzCcTarget, ModController, f32)>,
    cc_conditions: Vec<CcRange>,
    cc_triggers: Vec<CcRange>,
    crossfades: Vec<CcCrossfade>,
    crossfade_curve: CrossfadeCurve,
    set_cc: Vec<(u8, u8)>,
    reverb_send: Option<f32>,
    chorus_send: Option<f32>,
}

fn update_range(ranges: &mut Vec<CcRange>, cc: u8, high: bool, value: u8) {
    let range = match ranges.iter_mut().find(|r| r.cc == cc) {
        Some(range) => range,
        None => {
            ranges.push(CcRange { cc, lo: 0, hi: 127 });
            ranges.last_mut().unwrap()
        }
    };
    if high {
        range.hi = value;
    } else {
        range.lo = value;
    }
}

impl Default for RegionParamsBuilder {
    fn default() -> Self {
        RegionParamsBuilder {
            lovel: 0,
            hivel: 127,
            lokey: 0,
            hikey: 127,
            pitch_keycenter: 60,
            volume: 0,
            pan: 0,
            sample: None,
            default_path: None,
            loop_mode: LoopMode::NoLoop,
            loop_start: 0,
            loop_end: 0,
            offset: 0,
            cutoff: None,
            resonance: 0.0,
            amp_veltrack: 100.0,
            amp_keycenter: 60,
            amp_keytrack: 0.0,
            pan_veltrack: 0.0,
            pan_keycenter: 60,
            pan_keytrack: 0.0,
            fil_veltrack: 0,
            fil_keycenter: 60,
            fil_keytrack: 0,
            filter_type: FilterType::default(),
            ampeg_envelope: AmpegEnvelopeParams::default(),
            tune: 0,
            amplitude: 100.0,
            cc_mods: Vec::new(),
            cc_conditions: Vec::new(),
            cc_triggers: Vec::new(),
            crossfades: Vec::new(),
            crossfade_curve: CrossfadeCurve::default(),
            set_cc: Vec::new(),
            reverb_send: None,
            chorus_send: None,
        }
    }
}

impl RegionParamsBuilder {
    fn update_from_flag(&mut self, flag: SfzOpcode) {
        match flag {
            SfzOpcode::Lovel(val) => self.lovel = val,
            SfzOpcode::Hivel(val) => self.hivel = val,
            SfzOpcode::Key(val) => {
                self.lokey = val;
                self.hikey = val;
                self.pitch_keycenter = val;
            }
            SfzOpcode::Lokey(val) => self.lokey = val,
            SfzOpcode::Hikey(val) => self.hikey = val,
            SfzOpcode::PitchKeycenter(val) => self.pitch_keycenter = val,
            SfzOpcode::Pan(val) => self.pan = val,
            SfzOpcode::Volume(val) => self.volume = val,
            SfzOpcode::Sample(val) => self.sample = Some(val),
            SfzOpcode::LoopMode(val) => self.loop_mode = val,
            SfzOpcode::LoopStart(val) => self.loop_start = val,
            SfzOpcode::LoopEnd(val) => self.loop_end = val,
            SfzOpcode::Offset(val) => self.offset = val,
            SfzOpcode::Cutoff(val) => self.cutoff = Some(val),
            SfzOpcode::Resonance(val) => self.resonance = val,
            SfzOpcode::AmpVeltrack(val) => self.amp_veltrack = val,
            SfzOpcode::AmpKeytrack(val) => self.amp_keytrack = val,
            SfzOpcode::AmpKeycenter(val) => self.amp_keycenter = val,
            SfzOpcode::PanVeltrack(val) => self.pan_veltrack = val,
            SfzOpcode::PanKeytrack(val) => self.pan_keytrack = val,
            SfzOpcode::PanKeycenter(val) => self.pan_keycenter = val,
            SfzOpcode::FilVeltrack(val) => self.fil_veltrack = val,
            SfzOpcode::FilKeytrack(val) => self.fil_keytrack = val,
            SfzOpcode::FilKeycenter(val) => self.fil_keycenter = val,
            SfzOpcode::FilterType(val) => self.filter_type = val,
            SfzOpcode::DefaultPath(val) => self.default_path = Some(val),
            SfzOpcode::AmpegEnvelope(flag) => self.ampeg_envelope.update_from_flag(flag),
            SfzOpcode::Tune(val) => self.tune = val,
            SfzOpcode::Amplitude(val) => self.amplitude = val,
            SfzOpcode::CcMod {
                target,
                controller,
                value,
            } => {
                self.cc_mods
                    .retain(|(t, c, _)| !(*t == target && *c == controller));
                self.cc_mods.push((target, controller, value));
            }
            SfzOpcode::CcCondition { cc, high, value } => {
                update_range(&mut self.cc_conditions, cc, high, value)
            }
            SfzOpcode::CcTrigger { cc, high, value } => {
                update_range(&mut self.cc_triggers, cc, high, value)
            }
            SfzOpcode::CcCrossfade {
                cc,
                fade_in,
                high,
                value,
            } => {
                let xf = match self
                    .crossfades
                    .iter_mut()
                    .find(|xf| xf.cc == cc && xf.fade_in == fade_in)
                {
                    Some(xf) => xf,
                    None => {
                        // SFZ defaults: xfin_lo/hi = 0, xfout_lo/hi = 127
                        let default = if fade_in { 0 } else { 127 };
                        self.crossfades.push(CcCrossfade {
                            cc,
                            lo: default,
                            hi: default,
                            fade_in,
                        });
                        self.crossfades.last_mut().unwrap()
                    }
                };
                if high {
                    xf.hi = value;
                } else {
                    xf.lo = value;
                }
            }
            SfzOpcode::XfCcCurve(curve) => self.crossfade_curve = curve,
            SfzOpcode::SetCc(cc, value) => {
                self.set_cc.retain(|(c, _)| *c != cc);
                self.set_cc.push((cc, value));
            }
            SfzOpcode::ReverbSend(val) => self.reverb_send = Some(val),
            SfzOpcode::ChorusSend(val) => self.chorus_send = Some(val),
        }
    }

    fn build_modulation(&self) -> RegionModulation {
        let modulators = self
            .cc_mods
            .iter()
            .map(|&(target, controller, value)| {
                let (destination, amount) = match target {
                    SfzCcTarget::Amplitude => (ModDestination::AmplitudePercent, value),
                    SfzCcTarget::Volume => (ModDestination::VolumeDb, value),
                    SfzCcTarget::Pan => (ModDestination::Pan, value / 100.0),
                    SfzCcTarget::Pitch => (ModDestination::PitchCents, value),
                    SfzCcTarget::Cutoff => (ModDestination::CutoffCents, value),
                    SfzCcTarget::Resonance => (ModDestination::ResonanceDb, value),
                    SfzCcTarget::Envelope(env) => (ModDestination::EnvelopeLinear(env), value),
                };
                let mut source = ModSource::linear(controller);
                // The extended pitch bend controller is bipolar in SFZ.
                source.bipolar = controller == ModController::PitchWheel;
                RuntimeModulator {
                    source,
                    amount_source: None,
                    amount,
                    absolute: false,
                    destination,
                }
            })
            .collect();

        // Crossfades that can never attenuate are dropped so they cost nothing.
        let crossfades = self
            .crossfades
            .iter()
            .copied()
            .filter(|xf| {
                if xf.fade_in {
                    !(xf.lo == 0 && xf.hi == 0)
                } else {
                    !(xf.lo == 127 && xf.hi == 127)
                }
            })
            .collect();

        RegionModulation {
            modulators,
            crossfades,
            crossfade_curve: self.crossfade_curve,
            amplitude_percent: self.amplitude,
        }
    }

    fn build(self, base_path: &Path) -> Option<RegionParams> {
        let modulation = self.build_modulation();
        let relative_sample_path = if let Some(default_path) = self.default_path {
            PathBuf::from(default_path).join(self.sample?)
        } else {
            self.sample?.into()
        };

        let mut sample_path = base_path.join(relative_sample_path);
        match sample_path.canonicalize() {
            Ok(path) => sample_path = path,
            Err(_) => return None,
        }

        Some(RegionParams {
            velrange: self.lovel..=self.hivel,
            keyrange: self.lokey..=self.hikey,
            pitch_keycenter: self.pitch_keycenter,
            volume: self.volume,
            pan: self.pan,
            sample_path,
            loop_mode: self.loop_mode,
            loop_start: self.loop_start,
            loop_end: self.loop_end,
            offset: self.offset,
            cutoff: self.cutoff,
            resonance: self.resonance,
            amp_veltrack: self.amp_veltrack,
            amp_keycenter: self.amp_keycenter,
            amp_keytrack: self.amp_keytrack,
            pan_veltrack: self.pan_veltrack,
            pan_keycenter: self.pan_keycenter,
            pan_keytrack: self.pan_keytrack,
            fil_veltrack: self.fil_veltrack.clamp(-9600, 9600),
            fil_keycenter: self.fil_keycenter,
            fil_keytrack: self.fil_keytrack.clamp(0, 1200),
            filter_type: self.filter_type,
            ampeg_envelope: self.ampeg_envelope,
            tune: self.tune,
            amplitude: self.amplitude,
            modulation,
            cc_conditions: self.cc_conditions,
            cc_triggers: self.cc_triggers,
            set_cc: self.set_cc,
            reverb_send: self.reverb_send,
            chorus_send: self.chorus_send,
        })
    }
}

/// Structure that holds the opcode parameters of the SFZ file.
#[derive(Debug, Clone)]
pub struct RegionParams {
    pub velrange: RangeInclusive<u8>,
    pub keyrange: RangeInclusive<i8>,
    pub pitch_keycenter: i8,
    pub volume: i16,
    pub pan: i8,
    pub sample_path: PathBuf,
    pub loop_mode: LoopMode,
    pub loop_start: u32,
    pub loop_end: u32,
    pub offset: u32,
    pub cutoff: Option<f32>,
    pub resonance: f32,
    pub amp_veltrack: f32,
    pub amp_keycenter: i8,
    pub amp_keytrack: f32,
    pub pan_veltrack: f32,
    pub pan_keycenter: i8,
    pub pan_keytrack: f32,
    pub fil_veltrack: i16,
    pub fil_keycenter: i8,
    pub fil_keytrack: i16,
    pub filter_type: FilterType,
    pub ampeg_envelope: AmpegEnvelopeParams,
    pub tune: i16,
    /// Base amplitude in percent (`amplitude`).
    pub amplitude: f32,
    /// Controller-driven modulation (`*_onccN`, `xfin_*`/`xfout_*`).
    pub modulation: RegionModulation,
    /// Controller ranges required for the region to play (`loccN`/`hiccN`).
    pub cc_conditions: Vec<CcRange>,
    /// Controller ranges that trigger the region (`on_loccN`/`on_hiccN`).
    /// Regions with triggers are not played by notes.
    pub cc_triggers: Vec<CcRange>,
    /// Initial controller values (`set_ccN`).
    pub set_cc: Vec<(u8, u8)>,
    /// Reverb send in percent (`effect1`).
    pub reverb_send: Option<f32>,
    /// Chorus send in percent (`effect2`).
    pub chorus_send: Option<f32>,
}

fn get_group_level(group_type: SfzGroupType) -> Option<usize> {
    match group_type {
        SfzGroupType::Control => Some(1),
        SfzGroupType::Global => Some(2),
        SfzGroupType::Master => Some(3),
        SfzGroupType::Group => Some(4),
        SfzGroupType::Region => Some(5),
        SfzGroupType::Other => None,
    }
}

pub(super) fn parse_sf_root(
    tokens: impl Iterator<Item = SfzToken>,
    base_path: PathBuf,
) -> Vec<RegionParams> {
    let mut current_group = None;
    let mut group_data_stack = VecDeque::<RegionParamsBuilder>::new();
    let mut regions = Vec::new();

    for token in tokens {
        match token {
            SfzToken::Group(group) => {
                if current_group == Some(SfzGroupType::Region) {
                    let next_region = group_data_stack.pop_back().unwrap();
                    if let Some(built) = next_region.build(&base_path) {
                        regions.push(built);
                    }
                }

                if let Some(group_level) = get_group_level(group) {
                    current_group = Some(group);

                    while group_data_stack.len() < group_level {
                        let parent_group = group_data_stack.back().cloned().unwrap_or_default();
                        group_data_stack.push_back(parent_group);
                    }

                    while group_data_stack.len() > group_level {
                        group_data_stack.pop_back();
                    }
                } else {
                    current_group = None;
                }
            }
            SfzToken::Opcode(flag) => {
                if current_group.is_some() {
                    if let Some(group_data) = group_data_stack.back_mut() {
                        group_data.update_from_flag(flag);
                    }
                }
            }
        }
    }

    if current_group == Some(SfzGroupType::Region) {
        let next_region = group_data_stack.pop_back().unwrap();
        if let Some(built) = next_region.build(&base_path) {
            regions.push(built);
        }
    }

    regions
}
