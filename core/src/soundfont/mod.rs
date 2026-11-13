#![allow(non_camel_case_types)]
use std::{
    collections::{HashMap, HashSet},
    io,
    path::PathBuf,
    sync::Arc,
};

use biquad::Q_BUTTERWORTH_F32;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use thiserror::Error;
use xsynth_soundfonts::{
    convert_sample_index,
    modulation::{CcRange, ControllerMask, ModDestination, RegionModulation},
    FilterType, LoopMode,
};

use self::audio::load_audio_file;
pub use self::audio::AudioLoadError;

use super::{
    voice::VoiceControlData,
    voice::{EnvelopeParameters, Voice},
};
use crate::{helpers::db_to_amp, voice::EnvelopeDescriptor, AudioStreamParams, ChannelCount};

pub use xsynth_soundfonts::modulation;

pub use xsynth_soundfonts::{sf2::Sf2ParseError, sfz::SfzParseError};

mod audio;
mod config;
mod utils;
mod voice_spawners;
use utils::*;
use voice_spawners::*;

pub use config::*;

pub trait VoiceSpawner: Sync + Send {
    fn spawn_voice(&self, control: &VoiceControlData) -> Box<dyn Voice>;
    fn exclusive_class(&self) -> Option<u8> {
        None
    }

    /// Whether the voice may start with the current controller values
    /// (e.g. SFZ `loccN`/`hiccN`).
    fn can_spawn(&self, _control: &VoiceControlData) -> bool {
        true
    }

    /// Controllers that change the sound of voices from this spawner while they play.
    fn used_controllers(&self) -> ControllerMask {
        ControllerMask::EMPTY
    }

    /// Reverb and chorus send levels (0-1) requested by the soundfont region, if any.
    fn effect_sends(&self) -> (Option<f32>, Option<f32>) {
        (None, None)
    }
}

/// A voice spawner that is triggered by a controller instead of a note
/// (SFZ `on_loccN`/`on_hiccN`).
pub struct CcTriggeredSpawner {
    /// The controller range that triggers the voice when entered
    pub trigger: CcRange,
    /// The key the voice is played on
    pub key: u8,
    pub spawner: Box<dyn VoiceSpawner>,
}

pub trait SoundfontBase: Sync + Send + std::fmt::Debug {
    fn stream_params(&self) -> &'_ AudioStreamParams;

    fn get_attack_voice_spawners_at(
        &self,
        bank: u8,
        preset: u8,
        key: u8,
        vel: u8,
    ) -> Vec<Box<dyn VoiceSpawner>>;
    fn get_release_voice_spawners_at(
        &self,
        bank: u8,
        preset: u8,
        key: u8,
        vel: u8,
    ) -> Vec<Box<dyn VoiceSpawner>>;

    /// Voice spawners triggered by controller changes for the given program.
    fn get_cc_triggered_voice_spawners_at(
        &self,
        _bank: u8,
        _preset: u8,
    ) -> Vec<CcTriggeredSpawner> {
        Vec::new()
    }

    /// Initial controller values requested by the soundfont (SFZ `set_ccN`).
    fn default_controllers(&self) -> Vec<(u8, u8)> {
        Vec::new()
    }
}

#[derive(Clone)]
pub(super) struct LoopParams {
    pub mode: LoopMode,
    pub offset: u32,
    pub start: u32,
    pub end: u32,
    pub stop: Option<u32>,
}

struct SampleVoiceSpawnerParams {
    /// Base envelope, used to rebuild the envelope when note-on envelope modulators exist
    envelope_descriptor: EnvelopeDescriptor,
    envelope_options: EnvelopeOptions,
    /// Controller-driven modulation, `None` when the region has none
    modulation: Option<Arc<RegionModulation>>,
    /// Controller ranges required to start the voice
    cc_conditions: Arc<[CcRange]>,
    used_controllers: ControllerMask,
    reverb_send: Option<f32>,
    chorus_send: Option<f32>,
    volume: f32,
    pan: f32,
    speed_mult: f32,
    cutoff: Option<f32>,
    resonance: f32,
    filter_type: FilterType,
    loop_params: LoopParams,
    envelope: Arc<EnvelopeParameters>,
    sample: Arc<[Arc<[f32]>]>,
    interpolator: Interpolator,
    exclusive_class: Option<u8>,
}

pub(super) struct SoundfontInstrument {
    bank: u8,
    preset: u8,
    spawner_params_list: Vec<Vec<Arc<SampleVoiceSpawnerParams>>>,
    cc_triggered: Vec<(CcRange, u8, Arc<SampleVoiceSpawnerParams>)>,
}

/// Represents a sample soundfont to be used within XSynth.
///
/// Supports SFZ and SF2 soundfonts.
///
/// ## SFZ specification support (opcodes)
/// - `lovel` & `hivel`
/// - `lokey` & `hikey`
/// - `pitch_keycenter`
/// - `volume`
/// - `pan`
/// - `sample`
/// - `default_path`
/// - `loop_mode`
/// - `loop_start`
/// - `loop_end`
/// - `offset`
/// - `cutoff`
/// - `resonance`
/// - `fil_veltrack`
/// - `fil_keycenter`
/// - `fil_keytrack`
/// - `filter_type`
/// - `tune`
/// - `ampeg_start`
/// - `ampeg_delay`
/// - `ampeg_attack`
/// - `ampeg_hold`
/// - `ampeg_decay`
/// - `ampeg_sustain`
/// - `ampeg_release`
/// - `amplitude`
/// - `amplitude_onccN`, `volume_onccN`, `gain_onccN`, `pan_onccN`, `pitch_onccN`,
///   `tune_onccN`, `cutoff_onccN`, `resonance_onccN`, `cutoff_chanaft`
///   (and the SFZ v1 `*_ccN` aliases), updated while voices play
/// - `ampeg_*_onccN` / `ampeg_*ccN`, applied at note-on
/// - `loccN` & `hiccN`
/// - `on_loccN` & `on_hiccN`
/// - `set_ccN` & `set_hdccN`
/// - `xfin_loccN`, `xfin_hiccN`, `xfout_loccN`, `xfout_hiccN`, `xf_cccurve`
/// - `effect1` & `effect2` (reverb/chorus send, averaged per channel program)
///
/// ## SF2 specification support
/// ### Generators
/// - `startAddrsOffset`
/// - `endAddrsOffset`
/// - `startloopAddrsOffset`
/// - `endloopAddrsOffset`
/// - `startAddrsCoarseOffset`
/// - `endAddrsCoarseOffset`
/// - `startloopAddrsCoarseOffset`
/// - `endloopAddrsCoarseOffset`
/// - `initialFilterFc`
/// - `initialFilterQ`
/// - `pan`
/// - `delayVolEnv`
/// - `attackVolEnv`
/// - `holdVolEnv`
/// - `decayVolEnv`
/// - `sustainVolEnv`
/// - `releaseVolEnv`
/// - `instrument`
/// - `keyRange`
/// - `velRange`
/// - `initialAttenuation`
/// - `coarseTune`
/// - `fineTune`
/// - `sampleID`
/// - `sampleModes`
/// - `overridingRootKey`
/// - `scaleTuning`
/// - `exclusiveClass`
/// - `keynum`
/// - `velocity`
/// - `keynumToVolEnvHold`
/// - `keynumToVolEnvDecay`
///
/// ### Modulators
/// Key/velocity modulators are baked at soundfont load time. Modulators with
/// MIDI CC, channel pressure or pitch wheel sources are evaluated live, using
/// the same voice stages as unmodulated voices. Currently supported:
/// - sources: key number, note-on velocity, MIDI CCs, channel pressure, pitch wheel
/// - curves: linear, concave, convex, and switch
/// - transforms: linear and absolute
/// - destinations: attenuation, filter cutoff/Q, pan, volume envelope timings,
///   and pitch offsets
///
/// The default CC7, CC10, CC11, pitch wheel, CC91 and CC93 modulators are
/// implemented per channel.
///
/// The following SF2 features are intentionally not supported in the runtime
/// engine because they would add significant hot-path and binary-size cost:
/// - modulation envelope generators and destinations
/// - modulation LFO / vibrato LFO generators and destinations
/// - per-region chorus and reverb send generators
pub struct SampleSoundfont {
    instruments: Vec<SoundfontInstrument>,
    stream_params: AudioStreamParams,
    default_controllers: Vec<(u8, u8)>,
}

fn region_modulation_params(
    modulation: RegionModulation,
) -> (Option<Arc<RegionModulation>>, ControllerMask) {
    if modulation.is_empty() {
        (None, ControllerMask::EMPTY)
    } else {
        let mask = modulation.realtime_controllers();
        (Some(Arc::new(modulation)), mask)
    }
}

/// Errors that can be generated when loading an SFZ soundfont.
#[derive(Debug, Error)]
pub enum LoadSfzError {
    #[error("IO Error")]
    IOError(#[from] io::Error),

    #[error("Error loading samples")]
    AudioLoadError(#[from] AudioLoadError),

    #[error("Error parsing the SFZ: {0}")]
    SfzParseError(#[from] SfzParseError),
}

/// Errors that can be generated when loading a soundfont
/// of an unspecified format.
#[derive(Debug, Error)]
pub enum LoadSfError {
    #[error("Error loading the SFZ: {0}")]
    LoadSfzError(#[from] LoadSfzError),

    #[error("Error loading the SF2: {0}")]
    LoadSf2Error(#[from] Sf2ParseError),

    #[error("Unsupported format")]
    Unsupported,
}

impl SampleSoundfont {
    /// Loads a new sample soundfont of an unspecified type.
    /// The type of the soundfont will be determined from the file extension.
    ///
    /// Parameters:
    /// - `path`: The path of the soundfont to be loaded.
    /// - `stream_params`: Parameters of the output audio. See the `AudioStreamParams`
    ///   documentation for the available options.
    /// - `options`: The soundfont configuration. See the `SoundfontInitOptions`
    ///   documentation for the available options.
    pub fn new(
        path: impl Into<PathBuf>,
        stream_params: AudioStreamParams,
        options: SoundfontInitOptions,
    ) -> Result<Self, LoadSfError> {
        let path: PathBuf = path.into();
        if let Some(ext) = path.extension() {
            match ext.to_str().unwrap_or("").to_lowercase().as_str() {
                "sfz" => {
                    Self::new_sfz(path, stream_params, options).map_err(LoadSfError::LoadSfzError)
                }
                "sf2" => {
                    Self::new_sf2(path, stream_params, options).map_err(LoadSfError::LoadSf2Error)
                }
                _ => Err(LoadSfError::Unsupported),
            }
        } else {
            Err(LoadSfError::Unsupported)
        }
    }

    /// Loads a new SFZ soundfont
    ///
    /// Parameters:
    /// - `path`: The path of the SFZ soundfont to be loaded.
    /// - `stream_params`: Parameters of the output audio. See the `AudioStreamParams`
    ///   documentation for the available options.
    /// - `options`: The soundfont configuration. See the `SoundfontInitOptions`
    ///   documentation for the available options.
    pub fn new_sfz(
        sfz_path: impl Into<PathBuf>,
        stream_params: AudioStreamParams,
        options: SoundfontInitOptions,
    ) -> Result<Self, LoadSfzError> {
        let regions = xsynth_soundfonts::sfz::parse_soundfont(sfz_path.into())?;

        // Find the unique samples that we need to parse and convert
        let unique_sample_params: HashSet<_> = regions
            .iter()
            .map(sample_cache_from_region_params)
            .collect();

        // Parse and convert them in parallel
        let samples: Result<HashMap<_, _>, _> = unique_sample_params
            .into_par_iter()
            .map(|params| -> Result<(_, _), LoadSfzError> {
                let sample = load_audio_file(&params.path, stream_params)?;
                Ok((params, sample))
            })
            .collect();
        let samples = samples?;

        // Generate region params
        let mut spawner_params_list = Vec::<Vec<Arc<SampleVoiceSpawnerParams>>>::new();
        for _ in 0..(128 * 128) {
            spawner_params_list.push(Vec::new());
        }

        let mut cc_triggered = Vec::new();
        let mut default_controllers = Vec::<(u8, u8)>::new();

        // Write region params
        for region in regions {
            let params = sample_cache_from_region_params(&region);

            for &(cc, value) in &region.set_cc {
                default_controllers.retain(|(c, _)| *c != cc);
                default_controllers.push((cc, value));
            }

            let cc_trigger = region.cc_triggers.first().copied();

            // Key value -1 is used for regions that are only triggered by controllers
            if region.keyrange.contains(&-1) && cc_trigger.is_none() {
                continue;
            }

            let has_amplitude_mod = region
                .modulation
                .has_destination(|d| *d == ModDestination::AmplitudePercent);
            let amplitude_mult = if has_amplitude_mod {
                1.0
            } else {
                region.amplitude / 100.0
            };
            let (modulation, used_controllers) =
                region_modulation_params(region.modulation.clone());
            let cc_conditions: Arc<[CcRange]> = region.cc_conditions.clone().into();

            // Controller-triggered regions play once on their key center, at full velocity.
            let (keyrange, velrange) = if cc_trigger.is_some() {
                let key = region.pitch_keycenter.clamp(0, 127);
                (key..=key, 127..=127)
            } else {
                (region.keyrange.clone(), region.velrange.clone())
            };

            for key in keyrange {
                for vel in velrange.clone() {
                    let index = key_vel_to_index(key as u8, vel);
                    let speed_mult =
                        get_speed_mult_from_keys(key as u8, region.pitch_keycenter as u8)
                            * cents_factor(region.tune as f32);

                    let mut envelope = region.ampeg_envelope.clone();
                    envelope.ampeg_release +=
                        (vel as f32 / 127.0) * region.ampeg_envelope.ampeg_vel2release;
                    let envelope_descriptor = envelope_descriptor_from_region_params(&envelope);
                    let envelope_params = Arc::new(envelope_descriptor.to_envelope_params(
                        stream_params.sample_rate,
                        options.vol_envelope_options,
                    ));

                    let mut cutoff = None;
                    if options.use_effects {
                        if let Some(mut cutoff_t) = region.cutoff {
                            if cutoff_t >= 1.0 {
                                let cents = vel as f32 / 127.0 * region.fil_veltrack as f32
                                    + (key as f32 - region.fil_keycenter as f32)
                                        * region.fil_keytrack as f32;
                                cutoff_t *= cents_factor(cents);
                                cutoff = Some(
                                    cutoff_t
                                        .clamp(1.0, stream_params.sample_rate as f32 / 2.0 - 100.0),
                                );
                            }
                        }
                    }

                    let pan_mult = vel as f32 / 127.0 * region.pan_veltrack
                        + (key as f32 - region.pan_keycenter as f32) * region.pan_keytrack;
                    let pan = (region.pan as f32 + pan_mult).clamp(-100.0, 100.0) / 100.0;
                    let pan = (pan + 1.0) / 2.0;

                    let vol_vel = {
                        let a = region.amp_veltrack / 100.0;
                        let aabs = a.abs();
                        let vel = vel as f32;

                        127.0 * (1.0 - aabs)
                            + vel * (a + aabs) / 2.0
                            + (127.0 - vel) * (aabs - a) / 2.0
                    };
                    let vol_mult = (vol_vel / 127.0).powi(2);
                    let vol_db_add =
                        (key as f32 - region.amp_keycenter as f32) * region.amp_keytrack;
                    let vol_db = (region.volume as f32 + vol_db_add).clamp(-96.0, 12.0);
                    let volume = vol_mult * db_to_amp(vol_db) * amplitude_mult;

                    let sample_rate = samples[&params].1;

                    let loop_params = LoopParams {
                        mode: if region.loop_start == region.loop_end {
                            LoopMode::NoLoop
                        } else {
                            region.loop_mode
                        },
                        offset: convert_sample_index(
                            region.offset,
                            sample_rate,
                            stream_params.sample_rate,
                        ),
                        start: convert_sample_index(
                            region.loop_start,
                            sample_rate,
                            stream_params.sample_rate,
                        ),
                        end: convert_sample_index(
                            region.loop_end,
                            sample_rate,
                            stream_params.sample_rate,
                        ),
                        stop: None,
                    };

                    let mut region_samples = samples[&params].0.clone();
                    if stream_params.channels == ChannelCount::Stereo && region_samples.len() == 1 {
                        region_samples =
                            Arc::new([region_samples[0].clone(), region_samples[0].clone()]);
                    }

                    let spawner_params = Arc::new(SampleVoiceSpawnerParams {
                        envelope_descriptor,
                        envelope_options: options.vol_envelope_options,
                        modulation: modulation.clone(),
                        cc_conditions: cc_conditions.clone(),
                        used_controllers,
                        reverb_send: region.reverb_send.map(|v| v / 100.0),
                        chorus_send: region.chorus_send.map(|v| v / 100.0),
                        pan,
                        volume,
                        envelope: envelope_params,
                        speed_mult,
                        cutoff,
                        resonance: db_to_amp(region.resonance) * Q_BUTTERWORTH_F32,
                        filter_type: region.filter_type,
                        interpolator: options.interpolator,
                        loop_params,
                        sample: region_samples,
                        exclusive_class: None,
                    });

                    match cc_trigger {
                        Some(trigger) => {
                            cc_triggered.push((trigger, key as u8, spawner_params));
                        }
                        None => spawner_params_list[index].push(spawner_params),
                    }
                }
            }
        }

        Ok(SampleSoundfont {
            instruments: vec![SoundfontInstrument {
                bank: options.bank.unwrap_or(0),
                preset: options.preset.unwrap_or(0),
                spawner_params_list,
                cc_triggered,
            }],
            stream_params,
            default_controllers,
        })
    }

    /// Loads a new SF2 soundfont
    ///
    /// Parameters:
    /// - `path`: The path of the SF2 soundfont to be loaded.
    /// - `stream_params`: Parameters of the output audio. See the `AudioStreamParams`
    ///   documentation for the available options.
    /// - `options`: The soundfont configuration. See the `SoundfontInitOptions`
    ///   documentation for the available options.
    pub fn new_sf2(
        sf2_path: impl Into<PathBuf>,
        stream_params: AudioStreamParams,
        options: SoundfontInitOptions,
    ) -> Result<Self, Sf2ParseError> {
        let presets =
            xsynth_soundfonts::sf2::load_soundfont(sf2_path.into(), stream_params.sample_rate)?;

        let mut instruments = Vec::new();

        for preset in presets {
            if let Some(bank) = options.bank {
                if bank != preset.bank as u8 {
                    continue;
                }
            }
            if let Some(presetn) = options.preset {
                if presetn != preset.preset as u8 {
                    continue;
                }
            }

            let mut spawner_params_list = Vec::<Vec<Arc<SampleVoiceSpawnerParams>>>::new();
            for _ in 0..(128 * 128) {
                spawner_params_list.push(Vec::new());
            }

            let mut unique_envelope_params =
                Vec::<(EnvelopeDescriptor, Arc<EnvelopeParameters>)>::new();

            let mut last_modulation =
                None::<(RegionModulation, Arc<RegionModulation>, ControllerMask)>;
            let no_conditions: Arc<[CcRange]> = Arc::new([]);

            for region in preset.regions {
                for key in region.keyrange.clone() {
                    for vel in region.velrange.clone() {
                        let index = key_vel_to_index(key, vel);
                        let note_params = region.note_params(key, vel);
                        let runtime = region.runtime_modulation(key, vel);
                        let (modulation, used_controllers) = match &last_modulation {
                            Some((last, arc, mask)) if *last == runtime => {
                                (Some(arc.clone()), *mask)
                            }
                            _ => {
                                let (arc, mask) = region_modulation_params(runtime.clone());
                                if let Some(arc) = &arc {
                                    last_modulation = Some((runtime, arc.clone(), mask));
                                }
                                (arc, mask)
                            }
                        };
                        let envelope =
                            envelope_descriptor_from_region_params(&note_params.ampeg_envelope);
                        let envelope_params = if let Some((_, params)) = unique_envelope_params
                            .iter()
                            .find(|(descriptor, _)| *descriptor == envelope)
                        {
                            params.clone()
                        } else {
                            let params = Arc::new(envelope.to_envelope_params(
                                stream_params.sample_rate,
                                options.vol_envelope_options,
                            ));
                            unique_envelope_params.push((envelope, params.clone()));
                            params
                        };
                        let tuned_key_cents =
                            (key as f32 - region.root_key as f32) * region.scale_tuning as f32;
                        let speed_mult = cents_factor(
                            tuned_key_cents
                                + region.fine_tune as f32
                                + region.coarse_tune as f32 * 100.0
                                + note_params.tune_cents,
                        );

                        let mut cutoff = None;
                        if options.use_effects {
                            if let Some(cutoff_t) = note_params.cutoff {
                                if cutoff_t >= 1.0 {
                                    cutoff = Some(cutoff_t.clamp(
                                        1.0,
                                        stream_params.sample_rate as f32 / 2.0 - 100.0,
                                    ));
                                }
                            }
                        }

                        let pan = ((note_params.pan as f32 / 500.0) + 1.0) / 2.0;

                        let loop_params = LoopParams {
                            mode: if region.loop_start == region.loop_end {
                                LoopMode::NoLoop
                            } else {
                                region.loop_mode
                            },
                            offset: region.offset,
                            start: region.loop_start,
                            end: region.loop_end,
                            stop: Some(region.sample_end),
                        };

                        let mut region_samples = region.sample.clone();
                        if stream_params.channels == ChannelCount::Stereo
                            && region_samples.len() == 1
                        {
                            region_samples =
                                Arc::new([region_samples[0].clone(), region_samples[0].clone()]);
                        }

                        let spawner_params = Arc::new(SampleVoiceSpawnerParams {
                            envelope_descriptor: envelope,
                            envelope_options: options.vol_envelope_options,
                            modulation,
                            cc_conditions: no_conditions.clone(),
                            used_controllers,
                            reverb_send: None,
                            chorus_send: None,
                            pan,
                            volume: note_params.volume,
                            envelope: envelope_params,
                            speed_mult,
                            cutoff,
                            resonance: db_to_amp(note_params.resonance) * Q_BUTTERWORTH_F32,
                            filter_type: FilterType::LowPass,
                            interpolator: options.interpolator,
                            loop_params,
                            sample: region_samples,
                            exclusive_class: region.exclusive_class,
                        });

                        spawner_params_list[index].push(spawner_params.clone());
                    }
                }
            }

            let new = SoundfontInstrument {
                bank: preset.bank as u8,
                preset: preset.preset as u8,
                spawner_params_list,
                cc_triggered: Vec::new(),
            };
            instruments.push(new);
        }

        Ok(SampleSoundfont {
            instruments,
            stream_params,
            default_controllers: Vec::new(),
        })
    }
}

fn make_spawner<S: 'static + simdeez::Simd + Send + Sync>(
    params: &SampleVoiceSpawnerParams,
    key: u8,
    vel: u8,
    stream_params: &AudioStreamParams,
) -> Box<dyn VoiceSpawner> {
    match stream_params.channels {
        ChannelCount::Stereo => Box::new(StereoSampledVoiceSpawner::<S>::new(
            params,
            key,
            vel,
            *stream_params,
        )),
        ChannelCount::Mono => Box::new(MonoSampledVoiceSpawner::<S>::new(
            params,
            key,
            vel,
            *stream_params,
        )),
    }
}

impl SampleSoundfont {
    fn find_instrument(&self, bank: u8, preset: u8) -> Option<&SoundfontInstrument> {
        self.instruments
            .iter()
            .find(|i| i.bank == bank && i.preset == preset)
    }
}

impl std::fmt::Debug for SampleSoundfont {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "SampleSoundfont")
    }
}

impl SoundfontBase for SampleSoundfont {
    fn stream_params(&self) -> &'_ AudioStreamParams {
        &self.stream_params
    }

    fn get_attack_voice_spawners_at(
        &self,
        bank: u8,
        preset: u8,
        key: u8,
        vel: u8,
    ) -> Vec<Box<dyn VoiceSpawner>> {
        use simdeez::*; // nuts

        use simdeez::prelude::*;

        simd_runtime_generate!(
            fn get(
                key: u8,
                vel: u8,
                sf: &SoundfontInstrument,
                stream_params: &AudioStreamParams,
            ) -> Vec<Box<dyn VoiceSpawner>> {
                if sf.spawner_params_list.is_empty() {
                    return Vec::new();
                }

                let index = key_vel_to_index(key, vel);
                let mut vec = Vec::<Box<dyn VoiceSpawner>>::new();
                for spawner in &sf.spawner_params_list[index] {
                    vec.push(make_spawner::<S>(spawner, key, vel, stream_params));
                }
                vec
            }
        );

        match self.find_instrument(bank, preset) {
            Some(instrument) => get(key, vel, instrument, self.stream_params()),
            None => Vec::new(),
        }
    }

    fn get_cc_triggered_voice_spawners_at(&self, bank: u8, preset: u8) -> Vec<CcTriggeredSpawner> {
        use simdeez::*; // nuts

        use simdeez::prelude::*;

        simd_runtime_generate!(
            fn get(
                sf: &SoundfontInstrument,
                stream_params: &AudioStreamParams,
            ) -> Vec<CcTriggeredSpawner> {
                sf.cc_triggered
                    .iter()
                    .map(|(trigger, key, params)| CcTriggeredSpawner {
                        trigger: *trigger,
                        key: *key,
                        spawner: make_spawner::<S>(params, *key, 127, stream_params),
                    })
                    .collect()
            }
        );

        match self.find_instrument(bank, preset) {
            Some(instrument) if !instrument.cc_triggered.is_empty() => {
                get(instrument, self.stream_params())
            }
            _ => Vec::new(),
        }
    }

    fn default_controllers(&self) -> Vec<(u8, u8)> {
        self.default_controllers.clone()
    }

    fn get_release_voice_spawners_at(
        &self,
        _bank: u8,
        _preset: u8,
        _key: u8,
        _vel: u8,
    ) -> Vec<Box<dyn VoiceSpawner>> {
        vec![]
    }
}
