use crate::{handles::*, utils::*, XSynth_GenDefault_StreamParams, XSynth_StreamParams};
use xsynth_core::{
    channel::{ChannelConfigEvent, ChannelEvent, ChannelInitOptions},
    channel_group::{ChannelGroup, ChannelGroupConfig, SynthEvent},
    AudioPipe,
};

/// Options regarding which parts of the ChannelGroup should be multithreaded.
/// - channel: Render the MIDI channels parallel in a threadpool with the
///         specified thread count.
/// - key: Render the individisual keys of each channel parallel in a threadpool
///         with the specified thread count.
///
/// The following apply for all the values:
/// - A value of -1 means no multithreading.
/// - A value of 0 means that the thread count will be determined automatically.
#[repr(C)]
pub struct XSynth_ParallelismOptions {
    pub channel: i32,
    pub key: i32,
}

/// Generates the default values for the XSynth_ParallelismOptions struct
/// Default values are:
/// - channel: 0
/// - key: 0
#[no_mangle]
pub extern "C" fn XSynth_GenDefault_ParallelismOptions() -> XSynth_ParallelismOptions {
    XSynth_ParallelismOptions { channel: 0, key: 0 }
}

/// Options for initializing a ChannelGroup
/// - stream_params: Output parameters (see XSynth_StreamParams)
/// - channels: Number of MIDI channels. If this is set to 16 (MIDI standard),
///         then channel 10 will be configured for percussion.
/// - fade_out_killing: If set to true, the voices killed due to the voice limit
///         will fade out. If set to false, they will be killed immediately,
///         usually causing clicking but improving performance.
/// - parallelism: Options about the instance's parallelism
///         (see XSynth_ParallelismOptions)
#[repr(C)]
pub struct XSynth_GroupOptions {
    pub stream_params: XSynth_StreamParams,
    pub channels: u32,
    pub fade_out_killing: bool,
    pub parallelism: XSynth_ParallelismOptions,
}

/// Generates the default values for the XSynth_GroupOptions struct
/// Default values are:
/// - stream_params: Defaults for the XSynth_StreamParams struct
/// - channels: 16
/// - fade_out_killing: True
/// - parallelism: Defaults for the XSynth_ParallelismOptions struct
#[no_mangle]
pub extern "C" fn XSynth_GenDefault_GroupOptions() -> XSynth_GroupOptions {
    XSynth_GroupOptions {
        stream_params: XSynth_GenDefault_StreamParams(),
        channels: 16,
        fade_out_killing: true,
        parallelism: XSynth_GenDefault_ParallelismOptions(),
    }
}

/// Creates a new ChannelGroup. A ChannelGroup represents a MIDI synthesizer
/// within XSynth. It manages multiple MIDI channels at once.
///
/// --Parameters--
/// - options: The XSynth_GroupOptions struct which holds all the necessary
///         initialization settings for the channel group. A default configuration
///         can be generated using the XSynth_GenDefault_GroupOptions function.
///
/// --Returns--
/// This function will return the handle of the created channel group. This will
/// be necessary to use other XSynth_ChannelGroup_* functions, as they are specific
/// to each group.
#[no_mangle]
pub extern "C" fn XSynth_ChannelGroup_Create(options: XSynth_GroupOptions) -> XSynth_ChannelGroup {
    let channel_init_options = ChannelInitOptions {
        fade_out_killing: options.fade_out_killing,
    };

    let config = ChannelGroupConfig {
        channel_init_options,
        format: convert_synth_format(options.channels),
        audio_params: convert_streamparams_to_rust(options.stream_params),
        parallelism: convert_parallelism_to_rust(options.parallelism),
    };

    let new = ChannelGroup::new(config);
    XSynth_ChannelGroup::from(new)
}

/// Sends an audio event to a specific channel of the desired channel group.
///
/// --Parameters--
/// - handle: The handle of the channel group instance
/// - channel: The number of the MIDI channel to send the event to
///         (MIDI channel 1 is 0)
/// - event: The type of event to be sent (see below for available options)
/// - params: Parameters for the event
///
/// --Events--
/// - XSYNTH_AUDIO_EVENT_NOTEON: A MIDI note on event,
///         params: LOBYTE = key number (0-127), HIBYTE = velocity (0-127)
/// - XSYNTH_AUDIO_EVENT_NOTEOFF: A MIDI note on event
///         params: Key number (0-127)
/// - XSYNTH_AUDIO_EVENT_ALLNOTESOFF: Release all notes (No parameters)
/// - XSYNTH_AUDIO_EVENT_ALLNOTESKILLED: Kill all notes (No parameters)
/// - XSYNTH_AUDIO_EVENT_RESETCONTROL: Reset all control change data (No parameters)
/// - XSYNTH_AUDIO_EVENT_CONTROL: A MIDI control change event
///         params: LOBYTE = controller number, HIBYTE = controller value
/// - XSYNTH_AUDIO_EVENT_PROGRAMCHANGE: A MIDI program change event
///         params: preset number
/// - XSYNTH_AUDIO_EVENT_PITCH: Changes the pitch wheel position
///         params: pitch wheel position (0-16383, 8192=normal/middle)
/// - XSYNTH_AUDIO_EVENT_FINETUNE: Changes the fine tuning
///         params: fine tune value in cents (0-8192, 4096=normal/middle)
/// - XSYNTH_AUDIO_EVENT_COARSETUNE: Changes the coarse tuning
///         params: coarse tune value in semitones (0-128, 64=normal/middle)
#[no_mangle]
pub extern "C" fn XSynth_ChannelGroup_SendAudioEvent(
    handle: XSynth_ChannelGroup,
    channel: u32,
    event: u16,
    params: u16,
) {
    if let Ok(ev) = convert_audio_event(event, params) {
        handle.as_mut().send_event(SynthEvent::Channel(channel, ev));
    }
}

/// Sends an audio event to all channels of the desired channel group.
///
/// --Parameters--
/// - handle: The handle of the channel group instance
/// - event: The type of MIDI event sent (see XSynth_ChannelGroup_SendAudioEvent
///         for available options)
/// - params: Parameters for the event
#[no_mangle]
pub extern "C" fn XSynth_ChannelGroup_SendAudioEventAll(
    handle: XSynth_ChannelGroup,
    event: u16,
    params: u16,
) {
    if let Ok(ev) = convert_audio_event(event, params) {
        handle.as_mut().send_event(SynthEvent::AllChannels(ev));
    }
}

/// Sends a config event to a specific channel of the desired channel group.
///
/// --Parameters--
/// - handle: The handle of the channel group instance
/// - channel: The number of the MIDI channel to send the event to
///         (MIDI channel 1 is 0)
/// - event: The type of config event to be sent (see below for available options)
/// - params: Parameters for the event
///
/// --Events--
/// - XSYNTH_CONFIG_SETLAYERS: Sets the layer count for the channel.
///         params: The layer limit (0 = no limit, 1-.. = limit)
/// - XSYNTH_CONFIG_SETPERCUSSIONMODE: Controls whether the channel will be
///         standard or percussion.
///         params: 1 = set the channel to only use percussion patches,
///                 0 = set the channel to use standard patches
#[no_mangle]
pub extern "C" fn XSynth_ChannelGroup_SendConfigEvent(
    handle: XSynth_ChannelGroup,
    channel: u32,
    event: u16,
    params: u32,
) {
    if let Ok(ev) = convert_config_event(event, params) {
        handle.as_mut().send_event(SynthEvent::Channel(channel, ev));
    }
}

/// Sends a config event to all channels of the desired channel group.
///
/// --Parameters--
/// - handle: The handle of the channel group instance
/// - event: The type of config event to be sent (see
///         XSynth_ChannelGroup_SendConfigEvent for available options)
/// - params: Parameters for the event
#[no_mangle]
pub extern "C" fn XSynth_ChannelGroup_SendConfigEventAll(
    handle: XSynth_ChannelGroup,
    event: u16,
    params: u32,
) {
    if let Ok(ev) = convert_config_event(event, params) {
        handle.as_mut().send_event(SynthEvent::AllChannels(ev));
    }
}

/// Sets a list of soundfonts to be used in the desired channel group. To load
/// a new soundfont, see the XSynth_Soundfont_LoadNew function.
///
/// --Parameters--
/// - handle: The handle of the channel group instance
/// - sf_ids: Pointer to an array of soundfont handles
/// - count: The length of the above array
#[no_mangle]
pub unsafe extern "C" fn XSynth_ChannelGroup_SetSoundfonts(
    handle: XSynth_ChannelGroup,
    sf_ids: *const XSynth_Soundfont,
    count: u64,
) {
    unsafe {
        let ids = std::slice::from_raw_parts(sf_ids, count as usize);
        let sfvec = sfids_to_vec(ids);
        handle
            .as_mut()
            .send_event(SynthEvent::AllChannels(ChannelEvent::Config(
                ChannelConfigEvent::SetSoundfonts(sfvec),
            )));
    }
}

/// Removes all the soundfonts used in the desired channel group.
///
/// --Parameters--
/// - handle: The handle of the channel group instance
#[no_mangle]
pub extern "C" fn XSynth_ChannelGroup_ClearSoundfonts(handle: XSynth_ChannelGroup) {
    handle
        .as_mut()
        .send_event(SynthEvent::AllChannels(ChannelEvent::Config(
            ChannelConfigEvent::SetSoundfonts(Vec::new()),
        )));
}

/// Reads audio samples from the desired channel group. The amount of samples
/// determines the time of the current active MIDI events. For example if we
/// send a note on event and read 44100 samples (with a 44.1kHz sample rate),
/// then the note will be audible for 1 second. If after reading those samples
/// we send a note off event for the same key, then on the next read the key
/// will be released. If we don't, then the note will keep playing.
///
/// --Parameters--
/// - handle: The handle of the channel group instance
/// - buffer: Pointer to a mutable buffer to receive the audio samples. Each
///         item of the buffer should correspond to an audio sample of type
///         32bit float.
/// - length: Number of samples to read in the buffer
#[no_mangle]
pub unsafe extern "C" fn XSynth_ChannelGroup_ReadSamples(
    handle: XSynth_ChannelGroup,
    buffer: *mut f32,
    length: u64,
) {
    unsafe {
        if buffer.is_null() {
            return;
        }

        let slc = std::slice::from_raw_parts_mut(buffer, length as usize);
        handle.as_mut().read_samples(slc);
    }
}

/// Returns the active voice count of the desired channel group.
///
/// --Parameters--
/// - handle: The handle of the channel group instance
///
/// --Returns--
/// The active voice count as a 64bit unsigned integer
#[no_mangle]
pub extern "C" fn XSynth_ChannelGroup_VoiceCount(handle: XSynth_ChannelGroup) -> u64 {
    handle.as_ref().voice_count()
}

/// Returns the audio stream parameters of the desired channel group as an
/// XSynth_StreamParams struct. This may be useful when loading a new soundfont
/// which is meant to be used in that channel group.
///
/// --Parameters--
/// - handle: The handle of the channel group instance
///
/// --Returns--
/// This function returns an XSynth_StreamParams struct.
#[no_mangle]
pub extern "C" fn XSynth_ChannelGroup_GetStreamParams(
    handle: XSynth_ChannelGroup,
) -> XSynth_StreamParams {
    convert_streamparams_to_c(handle.as_ref().stream_params())
}

/// Drops the desired channel group.
///
/// --Parameters--
/// - handle: The handle of the channel group instance
#[no_mangle]
pub extern "C" fn XSynth_ChannelGroup_Drop(handle: XSynth_ChannelGroup) {
    handle.drop();
}
