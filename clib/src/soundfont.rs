use std::{
    ffi::{c_char, CStr},
    path::PathBuf,
    sync::Arc,
};

use xsynth_core::soundfont::{Interpolator, SampleSoundfont, SoundfontInitOptions};

use crate::{consts::*, handles::*, utils::*, XSynth_GenDefault_StreamParams, XSynth_StreamParams};

/// Options for the curves of a specific envelope.
/// - attack_curve: Controls the type of curve of the attack envelope stage.
///         See below for available options.
/// - decay_curve: Controls the type of curve of the decay envelope stage.
///         See below for available options.
/// - release_curve: Controls the type of curve of the release envelope stage.
///         See below for available options.
///
/// Available options:
/// - XSYNTH_ENVELOPE_CURVE_LINEAR: Apply a linear curve to the envelope stage.
///         This option is supported by the attack, decay and release stages.
/// - XSYNTH_ENVELOPE_CURVE_EXPONENTIAL: Apply an exponential curve to the
///         envelope stage. The decay and release stages will use a concave
///         curve, while the attack stage will use a convex curve.
#[repr(C)]
pub struct XSynth_EnvelopeOptions {
    pub attack_curve: u8,
    pub decay_curve: u8,
    pub release_curve: u8,
}

/// Generates the default values for the XSynth_EnvelopeOptions struct
/// Default values are:
/// - attack_curve: XSYNTH_ENVELOPE_CURVE_EXPONENTIAL
/// - decay_curve: XSYNTH_ENVELOPE_CURVE_LINEAR
/// - release_curve: XSYNTH_ENVELOPE_CURVE_LINEAR
#[no_mangle]
pub extern "C" fn XSynth_GenDefault_EnvelopeOptions() -> XSynth_EnvelopeOptions {
    XSynth_EnvelopeOptions {
        attack_curve: XSYNTH_ENVELOPE_CURVE_EXPONENTIAL,
        decay_curve: XSYNTH_ENVELOPE_CURVE_LINEAR,
        release_curve: XSYNTH_ENVELOPE_CURVE_LINEAR,
    }
}

/// Options for loading a new XSynth sample soundfont.
/// - stream_params: Output parameters (see XSynth_StreamParams)
/// - bank: The bank number (0-128) to extract and use from the soundfont
///         A value of -1 means to use all available banks (bank 0 for SFZ)
/// - preset: The preset number (0-127) to extract and use from the soundfont
///         A value of -1 means to use all available presets (preset 0 for SFZ)
/// - vol_envelope_options: Configures the volume envelope curves in dB units.
///         (see XSynth_EnvelopeOptions)
/// - use_effects: Whether or not to apply audio effects to the soundfont. Currently
///         only affecting the use of the cutoff filter. Setting to false may
///         improve performance slightly.
/// - interpolator: The type of interpolator to use for the new soundfont
///         Available values: INTERPOLATION_NEAREST (Nearest Neighbor interpolation),
///                           INTERPOLATION_LINEAR (Linear interpolation)
#[repr(C)]
pub struct XSynth_SoundfontOptions {
    pub stream_params: XSynth_StreamParams,
    pub bank: i16,
    pub preset: i16,
    pub vol_envelope_options: XSynth_EnvelopeOptions,
    pub use_effects: bool,
    pub interpolator: u16,
}

/// Generates the default values for the XSynth_SoundfontOptions struct
/// Default values are:
/// - stream_params: Defaults for the XSynth_StreamParams struct
/// - bank: -1
/// - preset: -1
/// - vol_envelope_options: Defaults for the XSynth_EnvelopeOptions struct
/// - use_effects: True
/// - interpolator: INTERPOLATION_NEAREST
#[no_mangle]
pub extern "C" fn XSynth_GenDefault_SoundfontOptions() -> XSynth_SoundfontOptions {
    XSynth_SoundfontOptions {
        stream_params: XSynth_GenDefault_StreamParams(),
        bank: -1,
        preset: -1,
        vol_envelope_options: XSynth_GenDefault_EnvelopeOptions(),
        use_effects: true,
        interpolator: XSYNTH_INTERPOLATION_NEAREST,
    }
}

/// Loads a new XSynth sample soundfont in memory.
///
/// --Parameters--
/// - path: The path of the soundfont to be loaded
/// - options: The soundfont initialization options
///         (XSynth_SoundfontOptions struct)
///
/// --Returns--
/// This function returns the handle of the loaded soundfont, which can be used
/// to send it to a channel group or realtime synth. If the soundfont fails to
/// load, the returned handle will contain a null pointer.
#[no_mangle]
pub unsafe extern "C" fn XSynth_Soundfont_LoadNew(
    path: *const c_char,
    options: XSynth_SoundfontOptions,
) -> XSynth_Soundfont {
    unsafe {
        let nullsf = XSynth_Soundfont {
            soundfont: std::ptr::null_mut(),
        };

        let path = match CStr::from_ptr(path).to_str() {
            Ok(path) => path,
            Err(..) => return nullsf,
        };
        let path = PathBuf::from(path);

        let sfinit = SoundfontInitOptions {
            bank: convert_program_value(options.bank.clamp(-1, 128)),
            preset: convert_program_value(options.preset.clamp(-1, 127)),
            vol_envelope_options: convert_envelope_to_rust(options.vol_envelope_options).unwrap(),
            use_effects: options.use_effects,
            interpolator: match options.interpolator {
                XSYNTH_INTERPOLATION_LINEAR => Interpolator::Linear,
                _ => Interpolator::Nearest,
            },
        };

        let stream_params = convert_streamparams_to_rust(options.stream_params);

        let new = match SampleSoundfont::new(path.clone(), stream_params, sfinit) {
            Ok(sf) => sf,
            Err(..) => return nullsf,
        };

        XSynth_Soundfont::from(Arc::new(new))
    }
}

/// Frees the handle of the desired soundfont.
///
/// Keep in mind that this does not free the memory the soundfont is
/// using. To clear the used memory the soundfont has to be unloaded/
/// replaced in the channel groups/realtime synthesizers where it was
/// sent. The following functions can be used for this purpose:
/// - XSynth_ChannelGroup_ClearSoundfonts
/// - XSynth_Realtime_ClearSoundfonts
///
/// To completely free the memory a soundfont is using you first need
/// to clear its handle and then remove it from any other places it is
/// being used.
///
/// --Parameters--
/// - handle: The handle of the soundfont
#[no_mangle]
pub extern "C" fn XSynth_Soundfont_Remove(handle: XSynth_Soundfont) {
    handle.drop();
}
