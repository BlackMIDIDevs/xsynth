use crate::{handles::*, utils::*, XSynth_ByteRange, XSynth_StreamParams};
use xsynth_core::{
    channel::{ChannelConfigEvent, ChannelEvent, ChannelInitOptions},
    channel_group::SynthEvent,
};
use xsynth_realtime::{RealtimeSynth, XSynthRealtimeConfig};

/// Options for initializing the XSynth Realtime module
/// - channels: Number of MIDI channels. If this is set to 16 (MIDI standard),
///         then channel 10 will be configured for percussion.
/// - multithreading: Render the individisual keys of each channel parallel in a
///         threadpool with the specified thread count. A value of -1 means no
///         multithreading, while a value of 0 means that the thread count will
///         be determined automatically.
/// - fade_out_killing: If set to true, the voices killed due to the voice limit
///         will fade out. If set to false, they will be killed immediately,
///         usually causing clicking but improving performance.
/// - render_window_ms: The length of the buffer reader in ms
/// - ignore_range: A range of velocities that will not be played
///         (see XSynth_ByteRange)
#[repr(C)]
pub struct XSynth_RealtimeConfig {
    pub channels: u32,
    pub multithreading: i32,
    pub fade_out_killing: bool,
    pub render_window_ms: f64,
    pub ignore_range: XSynth_ByteRange,
}

/// Generates the default values for the XSynth_RealtimeConfig struct
/// Default values are:
/// - channels: 16
/// - multithreading: -1
/// - fade_out_killing: False
/// - render_window_ms: 10.0ms
/// - ignore_range: 0->0 (Nothing ignored)
#[no_mangle]
pub extern "C" fn XSynth_GenDefault_RealtimeConfig() -> XSynth_RealtimeConfig {
    XSynth_RealtimeConfig {
        channels: 16,
        multithreading: -1,
        fade_out_killing: false,
        render_window_ms: 10.0,
        ignore_range: XSynth_ByteRange { start: 0, end: 0 },
    }
}

/// A struct that holds all the statistics the realtime module can
/// provide.
/// - voice_count: The amount of active voices
/// - buffer: Number of samples requested in the last read
/// - render_time: Percentage of the renderer load
#[repr(C)]
pub struct XSynth_RealtimeStats {
    pub voice_count: u64,
    pub buffer: i64,
    pub render_time: f64,
}

/// Initializes the XSynth Realtime module with the given configuration.
///
/// --Parameters--
/// - config: The initialization configuration (XSynth_RealtimeConfig struct)
///
/// --Returns--
/// This function will return the handle of the created realtime synthesizer.
/// This will be necessary to use other XSynth_Realtime_* functions, for the
/// specific synthesizer instance.
#[no_mangle]
pub extern "C" fn XSynth_Realtime_Create(config: XSynth_RealtimeConfig) -> XSynth_RealtimeSynth {
    let channel_init_options = ChannelInitOptions {
        fade_out_killing: config.fade_out_killing,
    };

    let options = XSynthRealtimeConfig {
        channel_init_options,
        render_window_ms: config.render_window_ms,
        format: convert_synth_format(config.channels),
        multithreading: convert_threadcount(config.multithreading),
        ignore_range: config.ignore_range.start..=config.ignore_range.end,
    };

    let new = RealtimeSynth::open_with_default_output(options);
    XSynth_RealtimeSynth::from(new)
}

/// Sends an raw u32 event to the desired realtime synth instance.
///
/// --Parameters--
/// - handle: The handle of the realtime synthesizer instance
/// - event: The raw u32 event to be sent
#[no_mangle]
pub extern "C" fn XSynth_Realtime_SendEventU32(handle: XSynth_RealtimeSynth, event: u32) {
    handle.as_mut().send_event_u32(event);
}

/// Sends an audio event to a specific channel of the desired realtime synth instance.
///
/// --Parameters--
/// - handle: The handle of the realtime synthesizer instance
/// - channel: The number of the MIDI channel to send the event to
///         (MIDI channel 1 is 0)
/// - event: The type of MIDI event sent (see XSynth_ChannelGroup_SendAudioEvent
///         for available options)
/// - params: Parameters for the event
#[no_mangle]
pub extern "C" fn XSynth_Realtime_SendAudioEvent(
    handle: XSynth_RealtimeSynth,
    channel: u32,
    event: u16,
    params: u16,
) {
    if let Ok(ev) = convert_audio_event(event, params) {
        handle.as_mut().send_event(SynthEvent::Channel(channel, ev));
    }
}

/// Sends an audio event to all channels of the desired realtime synth instance.
///
/// --Parameters--
/// - handle: The handle of the realtime synthesizer instance
/// - event: The type of MIDI event sent (see XSynth_ChannelGroup_SendAudioEvent
///         for available options)
/// - params: Parameters for the event
#[no_mangle]
pub extern "C" fn XSynth_Realtime_SendAudioEventAll(
    handle: XSynth_RealtimeSynth,
    event: u16,
    params: u16,
) {
    if let Ok(ev) = convert_audio_event(event, params) {
        handle.as_mut().send_event(SynthEvent::AllChannels(ev));
    }
}

/// Sends a config event to a specific channel of the desired realtime synth
/// instance.
///
/// --Parameters--
/// - handle: The handle of the realtime synthesizer instance
/// - channel: The number of the MIDI channel to send the event to
///         (MIDI channel 1 is 0)
/// - event: The type of config event to be sent (see
///         XSynth_ChannelGroup_SendConfigEvent for available options)
/// - params: Parameters for the event
#[no_mangle]
pub extern "C" fn XSynth_Realtime_SendConfigEvent(
    handle: XSynth_RealtimeSynth,
    channel: u32,
    event: u16,
    params: u32,
) {
    if let Ok(ev) = convert_config_event(event, params) {
        handle.as_mut().send_event(SynthEvent::Channel(channel, ev));
    }
}

/// Sends a config event to all channels of the desired realtime synth instance.
///
/// --Parameters--
/// - handle: The handle of the realtime synthesizer instance
/// - event: The type of config event to be sent (see
///         XSynth_ChannelGroup_SendConfigEvent for available options)
/// - params: Parameters for the event
#[no_mangle]
pub extern "C" fn XSynth_Realtime_SendConfigEventAll(
    handle: XSynth_RealtimeSynth,
    event: u16,
    params: u32,
) {
    if let Ok(ev) = convert_config_event(event, params) {
        handle.as_mut().send_event(SynthEvent::AllChannels(ev));
    }
}

/// Sets the length of the buffer reader to the desired value in ms.
///
/// --Parameters--
/// - handle: The handle of the realtime synthesizer instance
/// - render_window_ms: The length of the buffer reader in ms
#[no_mangle]
pub extern "C" fn XSynth_Realtime_SetBuffer(handle: XSynth_RealtimeSynth, render_window_ms: f64) {
    handle.as_ref().set_buffer(render_window_ms);
}

/// Sets the range of velocities that will be ignored.
///
/// --Parameters--
/// - handle: The handle of the realtime synthesizer instance
/// - ignore_range: The range. LOBYTE = start (0-127), HIBYTE = end (start-127)
#[no_mangle]
pub extern "C" fn XSynth_Realtime_SetIgnoreRange(
    handle: XSynth_RealtimeSynth,
    ignore_range: XSynth_ByteRange,
) {
    handle
        .as_mut()
        .get_sender_mut()
        .set_ignore_range(ignore_range.start..=ignore_range.end);
}

/// Sets a list of soundfonts to be used in the specified realtime synth
/// instance. To load a new soundfont, see the XSynth_Soundfont_LoadNew
/// function.
///
/// --Parameters--
/// - handle: The handle of the realtime synthesizer instance
/// - sf_ids: Pointer to an array of soundfont handles
/// - count: The length of the above array
///
/// --Returns--
/// This function returns the amount of soundfonts set.
#[no_mangle]
pub unsafe extern "C" fn XSynth_Realtime_SetSoundfonts(
    handle: XSynth_RealtimeSynth,
    sf_ids: *const XSynth_Soundfont,
    count: u64,
) -> u64 {
    unsafe {
        let ids = std::slice::from_raw_parts(sf_ids, count as usize);
        let sfvec = sfids_to_vec(ids);
        handle
            .as_mut()
            .send_event(SynthEvent::AllChannels(ChannelEvent::Config(
                ChannelConfigEvent::SetSoundfonts(sfvec),
            )));
        count
    }
}

/// Removes all the soundfonts used in the specified realtime synth instance.
///
/// --Parameters--
/// - handle: The handle of the realtime synthesizer instance
#[no_mangle]
pub extern "C" fn XSynth_Realtime_ClearSoundfonts(handle: XSynth_RealtimeSynth) {
    handle
        .as_mut()
        .send_event(SynthEvent::AllChannels(ChannelEvent::Config(
            ChannelConfigEvent::SetSoundfonts(Vec::new()),
        )));
}

/// Returns the audio stream parameters of the specified realtime synth
/// instance as an XSynth_StreamParams struct. This may be useful when loading
/// a new soundfont which is meant to be used here.
///
/// --Parameters--
/// - handle: The handle of the realtime synthesizer instance
///
/// --Returns--
/// This function returns an XSynth_StreamParams struct.
#[no_mangle]
pub extern "C" fn XSynth_Realtime_GetStreamParams(
    handle: XSynth_RealtimeSynth,
) -> XSynth_StreamParams {
    convert_streamparams_to_c(&handle.as_ref().stream_params())
}

/// Returns the statistics of the specified realtime synth instance.
///
/// --Parameters--
/// - handle: The handle of the realtime synthesizer instance
///
/// --Returns--
/// This function returns the statistic as an XSynth_RealtimeStats struct.
#[no_mangle]
pub extern "C" fn XSynth_Realtime_GetStats(handle: XSynth_RealtimeSynth) -> XSynth_RealtimeStats {
    let stats = handle.as_ref().get_stats();

    XSynth_RealtimeStats {
        voice_count: stats.voice_count(),
        buffer: stats.buffer().last_samples_after_read(),
        render_time: stats.buffer().average_renderer_load(),
    }
}

/// Resets the specified realtime synth instance. Kills all active notes
/// and resets all control change.
///
/// --Parameters--
/// - handle: The handle of the realtime synthesizer instance
#[no_mangle]
pub extern "C" fn XSynth_Realtime_Reset(handle: XSynth_RealtimeSynth) {
    handle.as_mut().get_sender_mut().reset_synth();
}

/// Drops the specified realtime synth instance.
///
/// --Parameters--
/// - handle: The handle of the realtime synthesizer instance
#[no_mangle]
pub extern "C" fn XSynth_Realtime_Drop(handle: XSynth_RealtimeSynth) {
    handle.drop();
}
