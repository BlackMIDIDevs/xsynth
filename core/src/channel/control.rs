use biquad::Q_BUTTERWORTH_F32;
use xsynth_soundfonts::modulation::ModController;

use crate::{
    helpers::{db_to_amp, FREQS},
    voice::VoiceControlData,
};

use super::{ChannelAudioEvent, ChannelEvent, ControlEvent, KeyNoteEvent, VoiceChannel};

pub(crate) struct ValueLerp {
    lerp_length: f32,
    step: f32,
    current: f32,
    end: f32,
}

impl ValueLerp {
    pub fn new(current: f32, sample_rate: u32) -> Self {
        Self {
            lerp_length: sample_rate as f32 * 0.01,
            step: 0.0,
            current,
            end: current,
        }
    }

    pub fn set_end(&mut self, end: f32) {
        self.step = (end - self.current) / self.lerp_length;
        self.end = end;
    }

    pub fn get_next(&mut self) -> f32 {
        if self.end > self.current {
            self.current = (self.current + self.step).min(self.end);
        } else if self.end < self.current {
            self.current = (self.current + self.step).max(self.end);
        }
        self.current
    }
}

/// The parameter targeted by data entry (CC6/CC38/CC96/CC97).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DataEntryTarget {
    None,
    Rpn(u8, u8),
    Nrpn(u8, u8),
}

/// Default vibrato rate in Hz (CC76 = 64).
const VIBRATO_RATE: f32 = 5.0;
/// Frames rendered between vibrato LFO updates.
pub(super) const VIBRATO_BLOCK_FRAMES: usize = 128;

pub(super) struct ControlEventData {
    rpn_msb: u8,
    rpn_lsb: u8,
    nrpn_msb: u8,
    nrpn_lsb: u8,
    data_target: DataEntryTarget,
    pitch_bend_sensitivity_lsb: u8,
    pitch_bend_sensitivity_msb: u8,
    pitch_bend_sensitivity: f32,
    pitch_bend_value: f32,
    pitch_bend_semitones: f32,
    fine_tune_lsb: u8,
    fine_tune_msb: u8,
    fine_tune_value: f32,
    coarse_tune_value: f32,
    mod_depth_range_msb: u8,
    mod_depth_range_lsb: u8,
    pub volume: ValueLerp, // 0.0 = silent, 1.0 = max volume
    pub pan: ValueLerp,    // 0.0 = left, 0.5 = center, 1.0 = right
    pub cutoff: Option<f32>,
    pub resonance: Option<f32>,
    pub expression: ValueLerp,

    /// Vibrato depth in cents from the modulation wheel at full deflection (RPN 5)
    mod_depth_range: f32,
    vibrato_rate: f32,
    vibrato_depth_mult: f32,
    vibrato_phase: f32,
    vibrato_cents: f32,

    pub damper: bool,
    pub hold2: bool,
    pub sostenuto: bool,
    pub portamento: bool,
    pub portamento_time: f32,
    pub portamento_control: Option<u8>,
    pub last_note: Option<u8>,
    pub mono: bool,

    /// Reverb send from CC91, 0-1
    pub reverb: f32,
    /// Chorus send from CC93, 0-1
    pub chorus: f32,
}

impl ControlEventData {
    pub fn new_defaults(sample_rate: u32) -> Self {
        ControlEventData {
            rpn_msb: 127,
            rpn_lsb: 127,
            nrpn_msb: 127,
            nrpn_lsb: 127,
            data_target: DataEntryTarget::None,
            pitch_bend_sensitivity_lsb: 0,
            pitch_bend_sensitivity_msb: 2,
            pitch_bend_sensitivity: 2.0,
            pitch_bend_value: 0.0,
            pitch_bend_semitones: 0.0,
            fine_tune_lsb: 0,
            fine_tune_msb: 64,
            fine_tune_value: 0.0,
            coarse_tune_value: 0.0,
            mod_depth_range_msb: 0,
            mod_depth_range_lsb: 64,
            volume: ValueLerp::new(1.0, sample_rate),
            pan: ValueLerp::new(0.5, sample_rate),
            cutoff: None,
            resonance: None,
            expression: ValueLerp::new(1.0, sample_rate),
            mod_depth_range: 50.0,
            vibrato_rate: VIBRATO_RATE,
            vibrato_depth_mult: 1.0,
            vibrato_phase: 0.0,
            vibrato_cents: 0.0,
            damper: false,
            hold2: false,
            sostenuto: false,
            portamento: false,
            portamento_time: 0.0,
            portamento_control: None,
            last_note: None,
            mono: false,
            reverb: 0.0,
            chorus: 0.0,
        }
    }
}

/// Maps CC5 (portamento time) to seconds, from 1ms (0) to 10s (127).
fn portamento_time(value: u8) -> f32 {
    if value == 0 {
        0.0
    } else {
        10f32.powf(value as f32 / 127.0 * 4.0 - 3.0)
    }
}

impl VoiceChannel {
    /// Sends a ControlEvent to the channel.
    /// See the `ControlEvent` documentation for more information.
    pub fn process_control_event(&mut self, event: ControlEvent) {
        match event {
            ControlEvent::Raw(controller, value) => {
                let controller = controller & 0x7F;
                let value = value.min(127);
                let old = self.voice_control_data.controllers.cc[controller as usize];
                self.voice_control_data.controllers.cc[controller as usize] = value;

                self.process_raw_cc(controller, value);

                if self
                    .params
                    .channel_sf
                    .used_controllers()
                    .contains(ModController::Cc(controller))
                {
                    self.propagate_voice_controls();
                }
                if old != value {
                    self.trigger_cc_voices(controller, old, value);
                }
            }
            ControlEvent::PitchBendSensitivity(sensitivity) => {
                let data = &mut self.control_event_data;
                data.pitch_bend_sensitivity = sensitivity;
                data.pitch_bend_semitones = sensitivity * data.pitch_bend_value;
                self.process_pitch();
            }
            ControlEvent::PitchBendValue(value) => {
                let data = &mut self.control_event_data;
                data.pitch_bend_value = value;
                data.pitch_bend_semitones = data.pitch_bend_sensitivity * value;
                self.voice_control_data.controllers.pitch_wheel = value.clamp(-1.0, 1.0);
                self.process_pitch();
            }
            ControlEvent::PitchBend(value) => {
                self.control_event_data.pitch_bend_semitones = value;
                self.process_pitch();
            }
            ControlEvent::FineTune(value) => {
                self.control_event_data.fine_tune_value = value;
                self.process_pitch();
            }
            ControlEvent::CoarseTune(value) => {
                self.control_event_data.coarse_tune_value = value;
                self.process_pitch();
            }
            ControlEvent::ChannelPressure(value) => {
                self.voice_control_data.controllers.channel_pressure = value.min(127);
                if self
                    .params
                    .channel_sf
                    .used_controllers()
                    .contains(ModController::ChannelPressure)
                {
                    self.propagate_voice_controls();
                }
            }
        }
    }

    fn process_raw_cc(&mut self, controller: u8, value: u8) {
        match controller {
            0x00 => {
                // Bank select
                self.params.set_bank(value);
            }
            0x4C => {
                // Vibrato rate, 64 = default rate
                self.control_event_data.vibrato_rate =
                    VIBRATO_RATE * 2f32.powf((value as f32 - 64.0) / 42.0);
            }
            0x4D => {
                // Vibrato depth, 64 = default depth
                self.control_event_data.vibrato_depth_mult = value as f32 / 64.0;
            }
            0x05 => {
                // Portamento time
                self.control_event_data.portamento_time = portamento_time(value);
            }
            0x06 | 0x26 => self.data_entry(controller, value),
            0x07 => {
                // Volume
                let vol: f32 = value as f32 / 128.0;
                self.control_event_data.volume.set_end(vol);
            }
            0x0A | 0x08 => {
                // Pan / Balance
                let pan: f32 = value as f32 / 128.0;
                self.control_event_data.pan.set_end(pan);
            }
            0x0B => {
                // Expression
                let expr = value as f32 / 128.0;
                self.control_event_data.expression.set_end(expr);
            }
            0x40 => {
                // Damper / Sustain
                self.control_event_data.damper = value >= 64;
                self.update_damper();
            }
            0x41 => {
                // Portamento on/off
                self.control_event_data.portamento = value >= 64;
            }
            0x42 => {
                // Sostenuto
                let sostenuto = value >= 64;
                if sostenuto != self.control_event_data.sostenuto {
                    self.control_event_data.sostenuto = sostenuto;
                    for key in self.key_voices.iter_mut() {
                        key.data.set_sostenuto(sostenuto);
                    }
                }
            }
            0x43 => {
                // Soft pedal, applied to new notes
                self.voice_control_data.note_gain = if value >= 64 { 0.7 } else { 1.0 };
            }
            0x45 => {
                // Hold 2
                self.control_event_data.hold2 = value >= 64;
                self.update_damper();
            }
            0x47 => {
                // Resonance
                if value > 64 {
                    let db = (value as f32 - 64.0) / 2.4;
                    let value = db_to_amp(db) * Q_BUTTERWORTH_F32;
                    self.control_event_data.resonance = Some(value);
                } else {
                    self.control_event_data.resonance = None;
                }
            }
            0x48 => {
                // Release
                self.voice_control_data.envelope.release = Some(value);
                self.propagate_voice_controls();
            }
            0x49 => {
                // Attack
                self.voice_control_data.envelope.attack = Some(value);
                self.propagate_voice_controls();
            }
            0x4A => {
                // Cutoff
                if value < 64 {
                    let value = value as usize + 64;
                    let mut freq = FREQS[value];
                    if freq > 7000.0 {
                        // I hate BASS
                        let mult = freq / 7000.0 - 1.0;
                        let mult = mult * 2.36 + 1.0;
                        freq = mult * 7000.0;
                    }
                    self.control_event_data.cutoff = Some(freq);
                } else {
                    self.control_event_data.cutoff = None;
                }
            }
            0x4B => {
                // Decay
                self.voice_control_data.envelope.decay = Some(value);
                self.propagate_voice_controls();
            }
            0x54 => {
                // Portamento control: the next note glides from this key
                self.control_event_data.portamento_control = Some(value);
            }
            0x5B => {
                // Reverb send
                self.control_event_data.reverb = value as f32 / 127.0;
            }
            0x5D => {
                // Chorus send
                self.control_event_data.chorus = value as f32 / 127.0;
            }
            0x60 => self.data_increment(1),
            0x61 => self.data_increment(-1),
            0x62 => {
                self.control_event_data.nrpn_lsb = value;
                self.select_nrpn();
            }
            0x63 => {
                self.control_event_data.nrpn_msb = value;
                self.select_nrpn();
            }
            0x64 => {
                self.control_event_data.rpn_lsb = value;
                self.select_rpn();
            }
            0x65 => {
                self.control_event_data.rpn_msb = value;
                self.select_rpn();
            }
            0x78 if value == 0 => {
                // All Sounds Off
                self.process_event(ChannelEvent::Audio(ChannelAudioEvent::AllNotesKilled));
            }
            0x79 if value == 0 => {
                // Reset All Controllers
                self.reset_control();
            }
            0x7B..=0x7D => {
                // All Notes Off, Omni Off, Omni On
                if controller != 0x7B || value == 0 {
                    self.process_event(ChannelEvent::Audio(ChannelAudioEvent::AllNotesOff));
                }
            }
            0x7E | 0x7F => {
                // Mono mode on / Poly mode on
                self.control_event_data.mono = controller == 0x7E;
                self.process_event(ChannelEvent::Audio(ChannelAudioEvent::AllNotesOff));
            }
            _ => {}
        }
    }

    fn select_rpn(&mut self) {
        let data = &mut self.control_event_data;
        data.data_target = if data.rpn_msb == 127 && data.rpn_lsb == 127 {
            DataEntryTarget::None
        } else {
            DataEntryTarget::Rpn(data.rpn_msb, data.rpn_lsb)
        };
    }

    fn select_nrpn(&mut self) {
        let data = &mut self.control_event_data;
        data.data_target = if data.nrpn_msb == 127 && data.nrpn_lsb == 127 {
            DataEntryTarget::None
        } else {
            DataEntryTarget::Nrpn(data.nrpn_msb, data.nrpn_lsb)
        };
    }

    /// Handles CC6 (data entry MSB) and CC38 (data entry LSB).
    fn data_entry(&mut self, controller: u8, value: u8) {
        let is_msb = controller == 0x06;
        match self.control_event_data.data_target {
            DataEntryTarget::None => {}
            DataEntryTarget::Rpn(0, param) => self.set_rpn(param, is_msb, value),
            DataEntryTarget::Rpn(..) => {}
            // GS/XG NRPNs (MSB 1) mirror the sound controllers, using the data entry MSB.
            DataEntryTarget::Nrpn(1, param) if is_msb => {
                let cc = match param {
                    0x08 => 0x4C, // vibrato rate
                    0x09 => 0x4D, // vibrato depth
                    0x0A => 0x4E, // vibrato delay
                    0x20 => 0x4A, // cutoff
                    0x21 => 0x47, // resonance
                    0x63 => 0x49, // attack
                    0x64 => 0x4B, // decay
                    0x66 => 0x48, // release
                    _ => return,
                };
                self.process_control_event(ControlEvent::Raw(cc, value));
            }
            DataEntryTarget::Nrpn(..) => {}
        }
    }

    /// Handles CC96 (data increment) and CC97 (data decrement) for registered parameters.
    fn data_increment(&mut self, delta: i8) {
        if let DataEntryTarget::Rpn(0, param) = self.control_event_data.data_target {
            let data = &self.control_event_data;
            let msb = match param {
                0 => data.pitch_bend_sensitivity_msb,
                1 => data.fine_tune_msb,
                2 => (data.coarse_tune_value + 64.0) as u8,
                5 => data.mod_depth_range_msb,
                _ => return,
            };
            let msb = (msb as i16 + delta as i16).clamp(0, 127) as u8;
            self.set_rpn(param, true, msb);
        }
    }

    fn set_rpn(&mut self, param: u8, is_msb: bool, value: u8) {
        let data = &mut self.control_event_data;
        match param {
            0 => {
                // Pitch bend sensitivity
                if is_msb {
                    data.pitch_bend_sensitivity_msb = value;
                } else {
                    data.pitch_bend_sensitivity_lsb = value;
                }
                let sensitivity = (data.pitch_bend_sensitivity_msb as f32)
                    + (data.pitch_bend_sensitivity_lsb as f32) / 100.0;
                self.process_control_event(ControlEvent::PitchBendSensitivity(sensitivity))
            }
            1 => {
                // Fine tune, 14 bit centered at 8192 = +-100 cents
                if is_msb {
                    data.fine_tune_msb = value;
                } else {
                    data.fine_tune_lsb = value;
                }
                let val = ((data.fine_tune_msb as u16) << 7) | data.fine_tune_lsb as u16;
                let val = (val as f32 - 8192.0) / 8192.0 * 100.0;
                self.process_control_event(ControlEvent::FineTune(val));
            }
            2 if is_msb => {
                // Coarse tune
                self.process_control_event(ControlEvent::CoarseTune(value as f32 - 64.0))
            }
            5 => {
                // Modulation depth range
                if is_msb {
                    data.mod_depth_range_msb = value;
                } else {
                    data.mod_depth_range_lsb = value;
                }
                data.mod_depth_range = data.mod_depth_range_msb as f32 * 100.0
                    + data.mod_depth_range_lsb as f32 * 100.0 / 128.0;
            }
            _ => {}
        }
    }

    fn update_damper(&mut self) {
        let damper = self.control_event_data.damper || self.control_event_data.hold2;
        for key in self.key_voices.iter_mut() {
            key.data.set_damper(damper);
        }
    }

    /// Starts controller-triggered voices (SFZ `on_loccN`/`on_hiccN`) whose
    /// range was just entered.
    fn trigger_cc_voices(&mut self, controller: u8, old: u8, new: u8) {
        for (index, trig) in self.params.channel_sf.cc_triggered().iter().enumerate() {
            let range = trig.trigger;
            if range.cc == controller && range.contains(new) && !range.contains(old) {
                if let Some(key) = self.key_voices.get_mut(trig.key as usize) {
                    key.event_cache
                        .push(KeyNoteEvent::CcTriggered(index as u16));
                }
            }
        }
    }

    /// Vibrato depth in cents from the modulation wheel and vibrato depth controllers.
    fn vibrato_depth(&self) -> f32 {
        let data = &self.control_event_data;
        let mod_wheel = self.voice_control_data.controllers.cc[0x01] as f32 / 127.0;
        mod_wheel * data.mod_depth_range * data.vibrato_depth_mult
    }

    /// Advances the channel vibrato LFO. Returns false when vibrato is inactive.
    pub(super) fn advance_vibrato(&mut self, frames: usize) -> bool {
        let depth = self.vibrato_depth();
        if depth <= 0.0 {
            if self.control_event_data.vibrato_cents != 0.0 {
                self.control_event_data.vibrato_cents = 0.0;
                self.control_event_data.vibrato_phase = 0.0;
                self.process_pitch();
            }
            return false;
        }

        let sample_rate = self.stream_params.sample_rate as f32;
        let data = &mut self.control_event_data;
        // Evaluate the LFO at the middle of the block
        let half = data.vibrato_rate * frames as f32 * 0.5 / sample_rate;
        let phase = (data.vibrato_phase + half).fract();
        data.vibrato_phase = (data.vibrato_phase + half * 2.0).fract();
        data.vibrato_cents = depth * (phase * std::f32::consts::TAU).sin();
        self.process_pitch();
        true
    }

    pub(super) fn vibrato_active(&self) -> bool {
        self.vibrato_depth() > 0.0 || self.control_event_data.vibrato_cents != 0.0
    }

    fn process_pitch(&mut self) {
        let data = &mut self.control_event_data;
        let combined = data.pitch_bend_semitones
            + data.coarse_tune_value
            + (data.fine_tune_value + data.vibrato_cents) / 100.0;

        self.voice_control_data.voice_pitch_multiplier = 2.0f32.powf(combined / 12.0);
        self.propagate_voice_controls();
    }

    /// Returns the portamento glide (source key, time in seconds) for a new
    /// note on the given key, if any.
    pub(super) fn take_portamento(&mut self, key: u8) -> Option<(u8, f32)> {
        let data = &mut self.control_event_data;
        let from = match data.portamento_control.take() {
            Some(from) => Some(from),
            None if data.portamento => data.last_note,
            None => None,
        };
        data.last_note = Some(key);
        match from {
            Some(from_key) if from_key != key && data.portamento_time > 0.0 => {
                Some((from_key, data.portamento_time))
            }
            _ => None,
        }
    }

    pub(super) fn reset_control(&mut self) {
        let mono = self.control_event_data.mono;
        let sostenuto = self.control_event_data.sostenuto;
        self.control_event_data = ControlEventData::new_defaults(self.stream_params.sample_rate);
        self.control_event_data.mono = mono;
        self.voice_control_data = VoiceControlData::new_defaults();
        self.propagate_voice_controls();

        for key in self.key_voices.iter_mut() {
            key.data.set_damper(false);
            if sostenuto {
                key.data.set_sostenuto(false);
            }
        }

        self.apply_default_controllers();
    }

    /// Applies the initial controller values requested by the soundfonts (SFZ `set_ccN`).
    pub(super) fn apply_default_controllers(&mut self) {
        let defaults = self.params.channel_sf.default_controllers().to_vec();
        for (cc, value) in defaults {
            self.process_control_event(ControlEvent::Raw(cc, value));
        }
    }

    pub(super) fn reset_program(&mut self) {
        self.params.set_bank(0);
        self.params.set_preset(0);
    }
}

#[cfg(test)]
mod tests {
    use crate::{AudioStreamParams, ChannelCount};

    use super::*;

    fn new_channel() -> VoiceChannel {
        VoiceChannel::new(
            Default::default(),
            AudioStreamParams::new(48_000, ChannelCount::Stereo),
            None,
        )
    }

    #[test]
    fn coarse_tune_only_uses_data_entry_msb() {
        let mut channel = new_channel();

        channel.process_control_event(ControlEvent::Raw(0x65, 0));
        channel.process_control_event(ControlEvent::Raw(0x64, 2));
        channel.process_control_event(ControlEvent::Raw(0x26, 100));

        assert_eq!(channel.control_event_data.coarse_tune_value, 0.0);

        channel.process_control_event(ControlEvent::Raw(0x06, 70));

        assert_eq!(channel.control_event_data.coarse_tune_value, 6.0);
    }

    #[test]
    fn reset_all_controllers_requires_zero_value() {
        let mut channel = new_channel();
        channel.process_control_event(ControlEvent::Raw(0x07, 32));
        channel.process_control_event(ControlEvent::Raw(0x79, 1));

        assert_eq!(channel.control_event_data.volume.end, 32.0 / 128.0);

        channel.process_control_event(ControlEvent::Raw(0x79, 0));

        assert_eq!(channel.control_event_data.volume.end, 1.0);
    }

    #[test]
    fn pitch_bend_survives_sensitivity_changes() {
        let mut channel = new_channel();
        channel.process_control_event(ControlEvent::PitchBendValue(0.5));
        assert_eq!(channel.control_event_data.pitch_bend_semitones, 1.0);

        // Changing the range must rescale the stored wheel position, not the product.
        channel.process_control_event(ControlEvent::Raw(0x65, 0));
        channel.process_control_event(ControlEvent::Raw(0x64, 0));
        channel.process_control_event(ControlEvent::Raw(0x06, 12));
        assert_eq!(channel.control_event_data.pitch_bend_semitones, 6.0);
        let expected = 2.0f32.powf(6.0 / 12.0);
        assert!((channel.voice_control_data.voice_pitch_multiplier - expected).abs() < 1e-6);
    }

    #[test]
    fn nrpn_does_not_change_rpn_parameters() {
        let mut channel = new_channel();
        channel.process_control_event(ControlEvent::Raw(0x65, 0));
        channel.process_control_event(ControlEvent::Raw(0x64, 0));
        channel.process_control_event(ControlEvent::Raw(0x63, 5));
        channel.process_control_event(ControlEvent::Raw(0x62, 5));
        channel.process_control_event(ControlEvent::Raw(0x06, 24));
        assert_eq!(channel.control_event_data.pitch_bend_sensitivity, 2.0);

        // RPN null deselects
        channel.process_control_event(ControlEvent::Raw(0x65, 127));
        channel.process_control_event(ControlEvent::Raw(0x64, 127));
        channel.process_control_event(ControlEvent::Raw(0x06, 24));
        assert_eq!(channel.control_event_data.pitch_bend_sensitivity, 2.0);
    }

    #[test]
    fn gs_nrpn_maps_to_sound_controllers() {
        let mut channel = new_channel();
        channel.process_control_event(ControlEvent::Raw(0x63, 1));
        channel.process_control_event(ControlEvent::Raw(0x62, 0x63));
        channel.process_control_event(ControlEvent::Raw(0x06, 20));
        assert_eq!(channel.voice_control_data.envelope.attack, Some(20));
    }

    #[test]
    fn data_increment_adjusts_pitch_bend_range() {
        let mut channel = new_channel();
        channel.process_control_event(ControlEvent::Raw(0x65, 0));
        channel.process_control_event(ControlEvent::Raw(0x64, 0));
        channel.process_control_event(ControlEvent::Raw(0x60, 0));
        assert_eq!(channel.control_event_data.pitch_bend_sensitivity, 3.0);
        channel.process_control_event(ControlEvent::Raw(0x61, 0));
        channel.process_control_event(ControlEvent::Raw(0x61, 0));
        assert_eq!(channel.control_event_data.pitch_bend_sensitivity, 1.0);
    }

    #[test]
    fn fine_tune_uses_14_bit_value() {
        let mut channel = new_channel();
        channel.process_control_event(ControlEvent::Raw(0x65, 0));
        channel.process_control_event(ControlEvent::Raw(0x64, 1));
        channel.process_control_event(ControlEvent::Raw(0x06, 96));
        assert_eq!(channel.control_event_data.fine_tune_value, 50.0);
        channel.process_control_event(ControlEvent::Raw(0x06, 0));
        channel.process_control_event(ControlEvent::Raw(0x26, 0));
        assert_eq!(channel.control_event_data.fine_tune_value, -100.0);
    }

    #[test]
    fn mod_wheel_enables_vibrato() {
        let mut channel = new_channel();
        assert!(!channel.vibrato_active());
        channel.process_control_event(ControlEvent::Raw(0x01, 127));
        assert!(channel.vibrato_active());
        assert_eq!(channel.vibrato_depth(), 50.0);

        // RPN 5: modulation depth range of 1 semitone
        channel.process_control_event(ControlEvent::Raw(0x65, 0));
        channel.process_control_event(ControlEvent::Raw(0x64, 5));
        channel.process_control_event(ControlEvent::Raw(0x06, 1));
        channel.process_control_event(ControlEvent::Raw(0x26, 0));
        assert_eq!(channel.vibrato_depth(), 100.0);

        // Advance a quarter period: the LFO swings the pitch
        assert!(channel.advance_vibrato(2400));
        assert!(channel.control_event_data.vibrato_cents.abs() > 1.0);

        channel.process_control_event(ControlEvent::Raw(0x01, 0));
        assert!(!channel.advance_vibrato(128));
        assert_eq!(channel.voice_control_data.voice_pitch_multiplier, 1.0);
    }

    #[test]
    fn effect_sends_follow_cc91_and_cc93() {
        let mut channel = new_channel();
        assert_eq!(channel.effect_sends(), (0.0, 0.0));
        channel.process_control_event(ControlEvent::Raw(0x5B, 127));
        channel.process_control_event(ControlEvent::Raw(0x5D, 0));
        assert_eq!(channel.effect_sends(), (1.0, 0.0));
    }

    #[test]
    fn portamento_glides_from_last_note() {
        let mut channel = new_channel();
        assert_eq!(channel.take_portamento(60), None);
        channel.process_control_event(ControlEvent::Raw(0x05, 64));
        channel.process_control_event(ControlEvent::Raw(0x41, 127));
        assert!(matches!(channel.take_portamento(64), Some((60, _))));
        // Portamento control overrides the source key
        channel.process_control_event(ControlEvent::Raw(0x54, 40));
        assert!(matches!(channel.take_portamento(50), Some((40, _))));
    }

    #[test]
    fn controllers_are_tracked_for_modulation() {
        let mut channel = new_channel();
        channel.process_control_event(ControlEvent::Raw(0x02, 99));
        channel.process_control_event(ControlEvent::ChannelPressure(42));
        channel.process_control_event(ControlEvent::PitchBendValue(-1.0));
        let c = &channel.voice_control_data.controllers;
        assert_eq!(c.cc[2], 99);
        assert_eq!(c.channel_pressure, 42);
        assert_eq!(c.pitch_wheel, -1.0);
    }
}
