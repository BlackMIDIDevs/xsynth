use crate::{
    consts::*, group::XSynth_ParallelismOptions, handles::*, soundfont::XSynth_EnvelopeOptions,
    XSynth_StreamParams,
};
use std::sync::Arc;
use xsynth_core::{
    channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent, ControlEvent},
    channel_group::{ParallelismOptions, SynthFormat, ThreadCount},
    soundfont::{EnvelopeCurveType, EnvelopeOptions, SoundfontBase},
    AudioStreamParams,
};

fn convert_envelope_curve(value: u8) -> Result<EnvelopeCurveType, ()> {
    match value {
        XSYNTH_ENVELOPE_CURVE_LINEAR => Ok(EnvelopeCurveType::Linear),
        XSYNTH_ENVELOPE_CURVE_EXPONENTIAL => Ok(EnvelopeCurveType::Exponential),
        _ => Err(()),
    }
}

pub(crate) fn convert_streamparams_to_rust(params: XSynth_StreamParams) -> AudioStreamParams {
    AudioStreamParams::new(params.sample_rate, params.audio_channels.into())
}

pub(crate) fn convert_threadcount(value: i32) -> ThreadCount {
    match value {
        ..=-1 => ThreadCount::None,
        0 => ThreadCount::Auto,
        v => ThreadCount::Manual(v as usize),
    }
}

pub(crate) fn convert_parallelism_to_rust(
    options: XSynth_ParallelismOptions,
) -> ParallelismOptions {
    ParallelismOptions {
        channel: convert_threadcount(options.channel),
        key: convert_threadcount(options.key),
    }
}

pub(crate) fn convert_envelope_to_rust(
    options: XSynth_EnvelopeOptions,
) -> Result<EnvelopeOptions, ()> {
    Ok(EnvelopeOptions {
        attack_curve: convert_envelope_curve(options.attack_curve)?,
        decay_curve: convert_envelope_curve(options.decay_curve)?,
        release_curve: convert_envelope_curve(options.release_curve)?,
    })
}

pub(crate) fn convert_streamparams_to_c(params: &AudioStreamParams) -> XSynth_StreamParams {
    XSynth_StreamParams {
        sample_rate: params.sample_rate,
        audio_channels: params.channels.count(),
    }
}

pub(crate) fn convert_audio_event(event: u16, params: u16) -> Result<ChannelEvent, ()> {
    let ev = match event {
        XSYNTH_AUDIO_EVENT_NOTEON => {
            let key = (params & 255) as u8;
            let vel = (params >> 8) as u8;
            ChannelAudioEvent::NoteOn { key, vel }
        }
        XSYNTH_AUDIO_EVENT_NOTEOFF => ChannelAudioEvent::NoteOff {
            key: (params & 255) as u8,
        },
        XSYNTH_AUDIO_EVENT_ALLNOTESKILLED => ChannelAudioEvent::AllNotesKilled,
        XSYNTH_AUDIO_EVENT_ALLNOTESOFF => ChannelAudioEvent::AllNotesOff,
        XSYNTH_AUDIO_EVENT_RESETCONTROL => ChannelAudioEvent::ResetControl,
        XSYNTH_AUDIO_EVENT_PROGRAMCHANGE => {
            let val = ((params & 255) as u8).clamp(0, 127);
            ChannelAudioEvent::ProgramChange(val)
        }
        XSYNTH_AUDIO_EVENT_CONTROL => {
            let val1 = ((params & 255) as u8).clamp(0, 127);
            let val2 = ((params >> 8) as u8).clamp(0, 127);
            ChannelAudioEvent::Control(ControlEvent::Raw(val1, val2))
        }
        XSYNTH_AUDIO_EVENT_PITCH => {
            let val = params.clamp(0, 16384) as f32;
            let val = (val - 8192.0) / 8192.0;
            ChannelAudioEvent::Control(ControlEvent::PitchBendValue(val))
        }
        XSYNTH_AUDIO_EVENT_FINETUNE => {
            let val = params.clamp(0, 8192) as f32;
            let val = (val - 4096.0) / 4096.0 * 100.0;
            ChannelAudioEvent::Control(ControlEvent::FineTune(val))
        }
        XSYNTH_AUDIO_EVENT_COARSETUNE => {
            let val = params.clamp(0, 128) as f32;
            ChannelAudioEvent::Control(ControlEvent::CoarseTune(val - 64.0))
        }
        XSYNTH_AUDIO_EVENT_SYSTEMRESET => ChannelAudioEvent::SystemReset,
        _ => return Err(()),
    };

    Ok(ChannelEvent::Audio(ev))
}

pub(crate) fn convert_config_event(event: u16, params: u32) -> Result<ChannelEvent, ()> {
    let ev = match event {
        XSYNTH_CONFIG_SETLAYERS => {
            let layers = convert_layers(params);
            ChannelConfigEvent::SetLayerCount(layers)
        }
        XSYNTH_CONFIG_SETPERCUSSIONMODE => {
            ChannelConfigEvent::SetPercussionMode(matches!(params, 1))
        }
        _ => return Err(()),
    };

    Ok(ChannelEvent::Config(ev))
}

pub(crate) unsafe fn sfids_to_vec(handles: &[XSynth_Soundfont]) -> Vec<Arc<dyn SoundfontBase>> {
    handles.iter().map(|handle| handle.clone()).collect()
}

fn convert_layers(layers: u32) -> Option<usize> {
    match layers {
        0 => None,
        v => Some(v as usize),
    }
}

pub(crate) fn convert_synth_format(channels: u32) -> SynthFormat {
    match channels {
        16 => SynthFormat::Midi,
        n => SynthFormat::Custom { channels: n },
    }
}

pub(crate) fn convert_program_value(val: i16) -> Option<u8> {
    if val < 0 {
        None
    } else {
        Some(val as u8)
    }
}
