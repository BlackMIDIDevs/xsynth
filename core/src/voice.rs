#![allow(dead_code)]
#![allow(non_camel_case_types)] // For the SIMD library

mod envelopes;
pub(crate) use envelopes::*;

mod simd;
pub(crate) use simd::*;

mod simdvoice;
pub(crate) use simdvoice::*;

mod base;
pub(crate) use base::*;

mod squarewave;
#[allow(unused_imports)]
pub(crate) use squarewave::*;

mod channels;
#[allow(unused_imports)]
pub(crate) use channels::*;

mod constant;
#[allow(unused_imports)]
pub(crate) use constant::*;

mod sampler;
pub(crate) use sampler::*;

mod control;
#[allow(unused_imports)]
pub(crate) use control::*;

mod cutoff;
pub(crate) use cutoff::*;

mod modulated;
pub(crate) use modulated::*;

use xsynth_soundfonts::modulation::ModController;

/// Options to modify the envelope of a voice.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct EnvelopeControlData {
    /// Controls the attack. Can take values from 0 to 128
    /// according to the MIDI CC spec.
    pub attack: Option<u8>,

    /// Controls the decay. Can take values from 0 to 128
    /// according to the MIDI CC spec.
    pub decay: Option<u8>,

    /// Controls the release. Can take values from 0 to 128
    /// according to the MIDI CC spec.
    pub release: Option<u8>,
}

/// The live controller values of a channel, used to evaluate
/// controller-driven soundfont modulators.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ControllerState {
    /// Raw MIDI CC values
    pub cc: [u8; 128],

    /// Pitch wheel position, between -1 and 1
    pub pitch_wheel: f32,

    /// Channel pressure (aftertouch), between 0 and 127
    pub channel_pressure: u8,
}

impl ControllerState {
    pub fn new_defaults() -> Self {
        let mut cc = [0; 128];
        cc[7] = 127;
        cc[10] = 64;
        cc[11] = 127;
        Self {
            cc,
            pitch_wheel: 0.0,
            channel_pressure: 0,
        }
    }

    /// Returns the value of a controller normalized to 0..1.
    #[inline]
    pub fn normalized(&self, controller: ModController) -> f32 {
        match controller {
            ModController::Cc(cc) => self.cc[(cc & 0x7F) as usize] as f32 / 127.0,
            ModController::PitchWheel => (self.pitch_wheel + 1.0) * 0.5,
            ModController::ChannelPressure => self.channel_pressure as f32 / 127.0,
        }
    }
}

impl Default for ControllerState {
    fn default() -> Self {
        Self::new_defaults()
    }
}

/// Portamento parameters for a newly started voice.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct PortamentoControl {
    /// The key the pitch glides from
    pub from_key: u8,

    /// Glide duration in seconds
    pub time: f32,
}

/// How a voice should be released.
#[derive(Copy, Clone, PartialEq)]
pub enum ReleaseType {
    /// Standard release. Uses the voice's envelope.
    Standard,

    /// Kills the voice with a fadeout of 1ms.
    Kill,
}

/// Options to control the parameters of a voice.
#[derive(Copy, Clone)]
pub struct VoiceControlData {
    /// Pitch multiplier
    pub voice_pitch_multiplier: f32,

    /// Envelope control
    pub envelope: EnvelopeControlData,

    /// Live controller values, for controller-driven soundfont modulation
    pub controllers: ControllerState,

    /// Portamento applied to a newly started voice. Only set while spawning
    /// a note that glides.
    pub portamento: Option<PortamentoControl>,

    /// Gain applied to newly started voices (soft pedal)
    pub note_gain: f32,
}

impl VoiceControlData {
    pub fn new_defaults() -> Self {
        VoiceControlData {
            voice_pitch_multiplier: 1.0,
            envelope: EnvelopeControlData::default(),
            controllers: ControllerState::new_defaults(),
            portamento: None,
            note_gain: 1.0,
        }
    }
}

pub trait VoiceGeneratorBase: Sync + Send {
    fn ended(&self) -> bool;
    fn signal_release(&mut self, rel_type: ReleaseType);
    fn process_controls(&mut self, control: &VoiceControlData);
}

pub trait VoiceSampleGenerator: VoiceGeneratorBase {
    fn render_to(&mut self, buffer: &mut [f32]);
}

pub trait Voice: VoiceSampleGenerator + Send + Sync {
    fn is_releasing(&self) -> bool;
    fn is_killed(&self) -> bool;

    fn velocity(&self) -> u8;
    fn exclusive_class(&self) -> Option<u8>;
}
