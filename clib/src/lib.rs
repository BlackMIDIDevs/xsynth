#![allow(clippy::missing_safety_doc)]
#![allow(clippy::result_unit_err)]

pub mod consts;
pub mod group;
pub mod handles;
pub mod realtime;
pub mod soundfont;
mod utils;

use consts::*;
use pkg_version::*;

const XSYNTH_VERSION: u32 =
    pkg_version_patch!() | pkg_version_minor!() << 8 | pkg_version_major!() << 16;

/// Returns the version of XSynth
///
/// --Returns--
/// The XSynth version. For example, 0x010102 (hex), would be version 1.1.2
#[no_mangle]
pub extern "C" fn XSynth_GetVersion() -> u32 {
    XSYNTH_VERSION
}

/// Parameters of the output audio
/// - sample_rate: Audio sample rate
/// - audio_channels: Number of audio channels
///         Supported: XSYNTH_AUDIO_CHANNELS_MONO (mono),
///                    XSYNTH_AUDIO_CHANNELS_STEREO (stereo)
#[repr(C)]
pub struct XSynth_StreamParams {
    pub sample_rate: u32,
    pub audio_channels: u16,
}

/// Generates the default values for the XSynth_StreamParams struct
/// Default values are:
/// - sample_rate = 44.1kHz
/// - audio_channels = XSYNTH_AUDIO_CHANNELS_STEREO
#[no_mangle]
pub extern "C" fn XSynth_GenDefault_StreamParams() -> XSynth_StreamParams {
    XSynth_StreamParams {
        sample_rate: 44100,
        audio_channels: XSYNTH_AUDIO_CHANNELS_STEREO,
    }
}
