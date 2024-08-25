#![allow(non_snake_case)]

use hotwatch::{Event, EventKind, Hotwatch};
use std::{
    ffi::c_void,
    os::raw::c_ulong,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use xsynth_core::channel::{ChannelConfigEvent, ChannelEvent};
use xsynth_realtime::{RealtimeEventSender, RealtimeSynth, SynthEvent};

#[cfg(windows)]
use winapi::{
    shared::{basetsd::DWORD_PTR, minwindef::DWORD, windef::HWND},
    um::{
        mmsystem::{
            CALLBACK_EVENT, CALLBACK_FUNCTION, CALLBACK_THREAD, CALLBACK_WINDOW, HMIDI, HMIDIOUT,
        },
        synchapi::SetEvent,
        winnt::HANDLE,
        winuser::{IsWindow, PostMessageW, PostThreadMessageW},
    },
};

mod parsers;
use parsers::*;

struct Synth {
    killed: Arc<Mutex<bool>>,
    stats_join_handle: thread::JoinHandle<()>,
    senders: RealtimeEventSender,
    hotwatch: Hotwatch,

    // This field is necessary to keep the synth loaded
    _synth: RealtimeSynth,
}

static mut GLOBAL_SYNTH: Option<Synth> = None;
static mut CURRENT_VOICE_COUNT: u64 = 0;

// region: Custom XSynth KDMAPI functions

/// This entire function is custom to xsynth and is not part of
/// the KDMAPI standard. Its basically just for testing.
#[no_mangle]
pub extern "C" fn GetVoiceCount() -> u64 {
    unsafe { CURRENT_VOICE_COUNT }
}

// endregion

// region: KDMAPI functions

#[no_mangle]
pub extern "C" fn InitializeKDMAPIStream() -> i32 {
    let config = Config::<Settings>::new().load().unwrap();
    let sflist = Config::<SFList>::new().load().unwrap();

    let realtime_synth = RealtimeSynth::open_with_default_output(config.get_synth_config());
    let mut sender = realtime_synth.get_senders();
    let params = realtime_synth.stream_params();

    sender.send_event(SynthEvent::AllChannels(ChannelEvent::Config(
        ChannelConfigEvent::SetLayerCount(config.get_layers()),
    )));
    sender.send_event(SynthEvent::AllChannels(ChannelEvent::Config(
        ChannelConfigEvent::SetSoundfonts(sflist.create_sfbase_vector(params)),
    )));

    let killed = Arc::new(Mutex::new(false));

    let stats = realtime_synth.get_stats();

    let killed_thread = killed.clone();
    let stats_join_handle = thread::spawn(move || {
        while !*killed_thread.lock().unwrap() {
            unsafe {
                CURRENT_VOICE_COUNT = stats.voice_count();
            }
            thread::sleep(Duration::from_millis(10));
        }
    });

    let mut hotwatch = Hotwatch::new_with_custom_delay(Duration::from_millis(500)).unwrap();

    // Watch for config changes and apply them
    let mut sender_thread = sender.clone();
    hotwatch
        .watch(Config::<Settings>::path(), move |event: Event| {
            if let EventKind::Modify(_) = event.kind {
                thread::sleep(Duration::from_millis(10));
                let layers = Config::<Settings>::new().load().unwrap().get_layers();
                sender_thread.send_event(SynthEvent::AllChannels(ChannelEvent::Config(
                    ChannelConfigEvent::SetLayerCount(layers),
                )));
            }
        })
        .unwrap();

    // Watch for soundfont list changes and apply them
    let mut sender_thread = sender.clone();
    hotwatch
        .watch(Config::<SFList>::path(), move |event: Event| {
            if let EventKind::Modify(_) = event.kind {
                thread::sleep(Duration::from_millis(10));
                let sfs = Config::<SFList>::new()
                    .load()
                    .unwrap()
                    .create_sfbase_vector(params);
                sender_thread.send_event(SynthEvent::AllChannels(ChannelEvent::Config(
                    ChannelConfigEvent::SetSoundfonts(sfs),
                )));
            }
        })
        .unwrap();

    unsafe {
        GLOBAL_SYNTH = Some(Synth {
            killed,
            senders: sender,
            stats_join_handle,
            hotwatch,
            _synth: realtime_synth,
        });
    }
    1
}

#[no_mangle]
pub extern "C" fn TerminateKDMAPIStream() -> i32 {
    unsafe {
        if let Some(mut synth) = GLOBAL_SYNTH.take() {
            *synth.killed.lock().unwrap() = true;
            synth.stats_join_handle.join().ok();

            synth.hotwatch.unwatch(Config::<Settings>::path()).unwrap();
            synth.hotwatch.unwatch(Config::<SFList>::path()).unwrap();
            Config::<Settings>::new()
                .repair()
                .expect("Error while saving settings");
            Config::<SFList>::new()
                .repair()
                .expect("Error while saving sf list");
            return 1;
        }
        0
    }
}

#[no_mangle]
pub extern "C" fn ResetKDMAPIStream() {
    unsafe {
        if let Some(synth) = GLOBAL_SYNTH.as_mut() {
            synth.senders.reset_synth();
        }
    }
}

#[no_mangle]
pub extern "C" fn SendDirectData(dwMsg: u32) -> u32 {
    unsafe {
        if let Some(sender) = GLOBAL_SYNTH.as_mut() {
            sender.senders.send_event_u32(dwMsg);
            return 1;
        }
        0
    }
}

#[no_mangle]
pub extern "C" fn SendDirectDataNoBuf(dwMsg: u32) -> u32 {
    SendDirectData(dwMsg)
}

#[no_mangle]
pub extern "C" fn IsKDMAPIAvailable() -> u32 {
    unsafe { GLOBAL_SYNTH.is_some() as u32 }
}

#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn ReturnKDMAPIVer(
    Major: *mut c_ulong,
    Minor: *mut c_ulong,
    Build: *mut c_ulong,
    Revision: *mut c_ulong,
) -> u32 {
    *Major = 4;
    *Minor = 1;
    *Build = 0;
    *Revision = 5;
    1
}

#[no_mangle]
pub extern "C" fn timeGetTime64() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

// endregion

// region: Unimplemented functions

#[no_mangle]
pub extern "C" fn DisableFeedbackMode() {}

#[no_mangle]
pub extern "C" fn SendCustomEvent(_eventtype: u32, _chan: u32, _param: u32) -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn SendDirectLongData() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn SendDirectLongDataNoBuf() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn PrepareLongData() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn UnprepareLongData() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn DriverSettings(
    _dwParam: c_ulong,
    _dwCmd: c_ulong,
    _lpValue: *mut c_void,
    _cbSize: c_ulong,
) -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn LoadCustomSoundFontsList(_Directory: u16) {}

#[no_mangle]
pub extern "C" fn GetDriverDebugInfo() {}

// endregion

// region: Callback functions for WINMM Wrapper (Windows Only)

cfg_if::cfg_if! {
  if #[cfg(windows)] {
    type CallbackFunction = unsafe extern "C" fn(HMIDIOUT, DWORD, DWORD_PTR, DWORD_PTR, DWORD_PTR);
    unsafe extern "C" fn def_callback(_: HMIDIOUT, _: DWORD, _: DWORD_PTR, _: DWORD_PTR, _: DWORD_PTR) {
    }
    static mut DUMMY_DEVICE: HMIDI = std::ptr::null_mut();
    static mut CALLBACK_INSTANCE: DWORD_PTR = 0;
    static mut CALLBACK: CallbackFunction = def_callback;
    static mut CALLBACK_TYPE: DWORD = 0;

    #[no_mangle]
    pub extern "C" fn modMessage() -> u32 {
        1
    }

    #[no_mangle]
    #[allow(clippy::missing_safety_doc)]
    pub unsafe extern "C" fn InitializeCallbackFeatures(
        OMHM: HMIDI,
        OMCB: CallbackFunction,
        OMI: DWORD_PTR,
        _OMU: DWORD_PTR,
        OMCM: DWORD,
    ) -> u32 {
        DUMMY_DEVICE = OMHM;
        CALLBACK = OMCB;
        CALLBACK_INSTANCE = OMI;
        CALLBACK_TYPE = OMCM;

        #[allow(clippy::fn_address_comparisons)]
        if OMCM == CALLBACK_WINDOW && CALLBACK != def_callback && IsWindow(CALLBACK as HWND) != 0 {
            return 0;
        }

        1
    }

    #[no_mangle]
    #[allow(clippy::missing_safety_doc)]
    pub unsafe extern "C" fn RunCallbackFunction(Msg: DWORD, P1: DWORD_PTR, P2: DWORD_PTR) {
        //We do a match case just to support stuff if needed
        match CALLBACK_TYPE {
            CALLBACK_FUNCTION => {
                CALLBACK(DUMMY_DEVICE as HMIDIOUT, Msg, P1, P2, CALLBACK_INSTANCE);
            }
            CALLBACK_EVENT => {
                SetEvent(CALLBACK as HANDLE);
            }
            CALLBACK_THREAD => {
                #[allow(clippy::fn_to_numeric_cast_with_truncation)]
                PostThreadMessageW(CALLBACK as DWORD, Msg, P1, P2.try_into().unwrap());
            }
            CALLBACK_WINDOW => {
                PostMessageW(CALLBACK as HWND, Msg, P1, P2.try_into().unwrap());
            }
            _ => println!("Type was NULL, Do Nothing"),
        }
    }
  }
}

// endregion
