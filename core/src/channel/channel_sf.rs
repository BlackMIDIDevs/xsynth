use std::{iter, ops::Deref, sync::Arc};

use xsynth_soundfonts::modulation::ControllerMask;

use crate::{
    helpers::are_arc_vecs_equal,
    soundfont::{CcTriggeredSpawner, SoundfontBase},
    voice::{Voice, VoiceControlData},
};

use super::voice_spawner::VoiceSpawnerMatrix;

#[derive(Default, PartialEq, Eq, Clone, Copy, Debug)]
pub struct ProgramDescriptor {
    pub bank: u8,
    pub preset: u8,
}

pub struct ChannelSoundfont {
    soundfonts: Vec<Arc<dyn SoundfontBase>>,
    matrix: VoiceSpawnerMatrix,
    curr_program: ProgramDescriptor,
    /// Controllers that modulate voices of the current program while they play
    used_controllers: ControllerMask,
    /// Reverb and chorus send scale requested by the current program
    effect_scale: (f32, f32),
    cc_triggered: Vec<CcTriggeredSpawner>,
    default_controllers: Vec<(u8, u8)>,
}

impl Deref for ChannelSoundfont {
    type Target = VoiceSpawnerMatrix;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.matrix
    }
}

impl ChannelSoundfont {
    pub fn new() -> Self {
        ChannelSoundfont {
            soundfonts: Vec::new(),
            matrix: VoiceSpawnerMatrix::new(),
            curr_program: Default::default(),
            used_controllers: ControllerMask::EMPTY,
            effect_scale: (1.0, 1.0),
            cc_triggered: Vec::new(),
            default_controllers: Vec::new(),
        }
    }

    /// Sets the soundfonts. Returns true if they changed.
    pub fn set_soundfonts(&mut self, soundfonts: Vec<Arc<dyn SoundfontBase>>) -> bool {
        if !are_arc_vecs_equal(&self.soundfonts, &soundfonts) {
            self.soundfonts = soundfonts;
            // Earlier soundfonts take priority, so apply them last.
            let mut defaults = Vec::<(u8, u8)>::new();
            for sf in self.soundfonts.iter().rev() {
                for (cc, value) in sf.default_controllers() {
                    defaults.retain(|(c, _)| *c != cc);
                    defaults.push((cc, value));
                }
            }
            self.default_controllers = defaults;
            self.rebuild_matrix();
            true
        } else {
            false
        }
    }

    #[inline(always)]
    pub fn used_controllers(&self) -> &ControllerMask {
        &self.used_controllers
    }

    #[inline(always)]
    pub fn effect_scale(&self) -> (f32, f32) {
        self.effect_scale
    }

    #[inline(always)]
    pub fn cc_triggered(&self) -> &[CcTriggeredSpawner] {
        &self.cc_triggered
    }

    pub fn default_controllers(&self) -> &[(u8, u8)] {
        &self.default_controllers
    }

    pub fn change_program(&mut self, program: ProgramDescriptor) {
        if self.curr_program != program {
            self.curr_program = program;
            self.rebuild_matrix();
        }
    }

    fn rebuild_matrix(&mut self) {
        // If a preset/instr. is missing from all banks it will be muted,
        // if a preset/instr. has regions in bank 0, all missing banks will be replaced by 0,
        // if a preset/instr. has regions in any bank other than 0, all missing banks will be muted.
        // For drum patches the same applies with bank and preset switched.

        let bank = self.curr_program.bank;
        let preset = self.curr_program.preset;

        let mut used = ControllerMask::EMPTY;
        let (mut reverb_sum, mut reverb_count) = (0.0f64, 0usize);
        let (mut chorus_sum, mut chorus_count) = (0.0f64, 0usize);

        for k in 0..128u8 {
            for v in 0..128u8 {
                let find_replacement_attack = || {
                    if bank == 128 {
                        self.soundfonts
                            .iter()
                            .map(|sf| sf.get_attack_voice_spawners_at(bank, 0, k, v))
                            .find(|vec| !vec.is_empty())
                    } else {
                        self.soundfonts
                            .iter()
                            .map(|sf| sf.get_attack_voice_spawners_at(0, preset, k, v))
                            .find(|vec| !vec.is_empty())
                    }
                };

                let attack_spawners = self
                    .soundfonts
                    .iter()
                    .map(|sf| sf.get_attack_voice_spawners_at(bank, preset, k, v))
                    .chain(iter::once_with(find_replacement_attack).flatten())
                    .find(|vec| !vec.is_empty())
                    .unwrap_or_default();

                let find_replacement_release = || {
                    if bank == 128 {
                        self.soundfonts
                            .iter()
                            .map(|sf| sf.get_release_voice_spawners_at(bank, 0, k, v))
                            .find(|vec| !vec.is_empty())
                    } else {
                        self.soundfonts
                            .iter()
                            .map(|sf| sf.get_release_voice_spawners_at(0, preset, k, v))
                            .find(|vec| !vec.is_empty())
                    }
                };

                let release_spawners = self
                    .soundfonts
                    .iter()
                    .map(|sf| sf.get_release_voice_spawners_at(bank, preset, k, v))
                    .chain(iter::once_with(find_replacement_release).flatten())
                    .find(|vec| !vec.is_empty())
                    .unwrap_or_default();

                for spawner in attack_spawners.iter().chain(release_spawners.iter()) {
                    used = used.union(&spawner.used_controllers());
                    let (reverb, chorus) = spawner.effect_sends();
                    if let Some(reverb) = reverb {
                        reverb_sum += reverb as f64;
                        reverb_count += 1;
                    }
                    if let Some(chorus) = chorus {
                        chorus_sum += chorus as f64;
                        chorus_count += 1;
                    }
                }

                self.matrix.set_spawners_attack(k, v, attack_spawners);
                self.matrix.set_spawners_release(k, v, release_spawners);
            }
        }

        self.cc_triggered = self
            .soundfonts
            .iter()
            .map(|sf| sf.get_cc_triggered_voice_spawners_at(bank, preset))
            .find(|vec| !vec.is_empty())
            .unwrap_or_default();
        for trig in &self.cc_triggered {
            used = used.union(&trig.spawner.used_controllers());
        }

        // Per-voice sends would need a second output bus per voice, so the region
        // send levels are averaged into one scale per channel program.
        let average = |sum: f64, count: usize| {
            if count == 0 {
                1.0
            } else {
                (sum / count as f64) as f32
            }
        };
        self.effect_scale = (
            average(reverb_sum, reverb_count),
            average(chorus_sum, chorus_count),
        );
        self.used_controllers = used;
    }

    pub fn spawn_voices_attack<'a>(
        &'a self,
        control: &'a VoiceControlData,
        key: u8,
        vel: u8,
    ) -> impl Iterator<Item = Box<dyn Voice>> + 'a {
        self.matrix.spawn_voices_attack(control, key, vel)
    }

    pub fn spawn_voices_release<'a>(
        &'a self,
        control: &'a VoiceControlData,
        key: u8,
        vel: u8,
    ) -> impl Iterator<Item = Box<dyn Voice>> + 'a {
        self.matrix.spawn_voices_release(control, key, vel)
    }

    pub fn exclusive_classes_attack<'a>(
        &'a self,
        key: u8,
        vel: u8,
    ) -> impl Iterator<Item = u8> + 'a {
        self.matrix.exclusive_classes_attack(key, vel)
    }
}
