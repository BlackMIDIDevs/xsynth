use std::{ffi::c_void, os::raw::c_ulong, sync::Arc, thread, time::Duration};

use core::{
  channel::ChannelEvent,
  soundfont::{SoundfontBase, SquareSoundfont},
};
use cpal::traits::{DeviceTrait, HostTrait};
use realtime::{RealtimeEventSender, RealtimeSynth, SynthEvent};
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

struct Synth
{
  senders: RealtimeEventSender,
  synth: RealtimeSynth,
}

static mut synth: Option<Synth> = None;
static mut voice_count: u64 = 0;

//-------------------------------------------------------------------------------------------------
// Custom xsynth KDMAPI functions
//-------------------------------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn GetVoiceCount() -> u64 //This entire function is custom to xsynth and is not part of the kdmapi standard. Its basically just for testing
{
  unsafe {
    //println!("Voice Count: {}", voice_count);
    voice_count
  }
}

//-------------------------------------------------------------------------------------------------
// KDMAPI functions
//-------------------------------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn InitializeKDMAPIStream() -> i32
{
  let realtime_synth = RealtimeSynth::open_with_all_defaults();
  let mut sender = realtime_synth.get_senders();

  let params = realtime_synth.stream_params();

  let soundfonts: Vec<Arc<dyn SoundfontBase>> = vec![Arc::new(SquareSoundfont::new(
    params.sample_rate,
    params.channels,
  ))];

  sender.send_event(SynthEvent::AllChannels(ChannelEvent::SetSoundfonts(
    soundfonts,
  )));

  let stats = realtime_synth.get_stats();
  unsafe {
    thread::spawn(move || {
      loop
      {
        voice_count = stats.voice_count();
        thread::sleep(Duration::from_millis(10));
      }
    });
  }

  unsafe {
    synth = Some(Synth {
      senders: sender,
      synth: realtime_synth,
    });
  }
  1
}

#[no_mangle]
pub extern "C" fn TerminateKDMAPIStream() -> i32
{
  unsafe {
    synth = None;
  }
  println!("TerminateKDMAPIStream");
  //std::process::exit(0) //Currently a workaround for chikara not closing
  1
}

#[no_mangle]
pub extern "C" fn ResetKDMAPIStream()
{
  println!("ResetKDMAPIStream");
  //Just terminate and reinitialize
  TerminateKDMAPIStream();
  InitializeKDMAPIStream();
}

#[no_mangle]
pub extern "C" fn SendDirectData(dwMsg: u32) -> u32
{
  unsafe {
    match synth.as_mut()
    {
      Some(sender) =>
      {
        sender.senders.send_event_u32(dwMsg);
      }
      None =>
      {}
    }
  }
  1
}

#[no_mangle]
pub extern "C" fn SendDirectDataNoBuf(dwMsg: u32) -> u32
{
  SendDirectData(dwMsg); //We don't have a buffer, just use SendDirectData
  1
}

#[no_mangle]
pub extern "C" fn IsKDMAPIAvailable() -> u32
{
  println!("IsKDMAPIAvailable");
  1 //Yes, we are available
}

//-------------------------------------------------------------------------------------------------
// Unimplemented functions
//-------------------------------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn DisableFeedbackMode()
{
  println!("DisableFeedbackMode");
}

#[no_mangle]
pub extern "C" fn SendCustomEvent(eventtype: u32, chan: u32, param: u32) -> u32
{
  println!("SendCustomEvent");
  1
}

#[no_mangle]
pub extern "C" fn SendDirectLongData() -> u32
{
  println!("SendDirectLongData");
  1
}

#[no_mangle]
pub extern "C" fn SendDirectLongDataNoBuf() -> u32
{
  println!("SendDirectLongDataNoBuf");
  1
}

#[no_mangle]
pub extern "C" fn PrepareLongData() -> u32
{
  println!("PrepareLongData");
  1
}

#[no_mangle]
pub extern "C" fn UnprepareLongData() -> u32
{
  println!("UnprepareLongData");
  1
}

#[no_mangle]
pub extern "C" fn DriverSettings(
  dwParam: c_ulong,
  dwCmd: c_ulong,
  lpValue: *mut c_void,
  cbSize: c_ulong,
) -> u32
{
  println!("DriverSettings");
  1
}

#[no_mangle]
pub extern "C" fn LoadCustomSoundFontsList(Directory: String)
{
  println!("LoadCustomSoundFontsList");
}

#[no_mangle]
pub extern "C" fn GetDriverDebugInfo()
{
  println!("GetDriverDebugInfo");
}

//-------------------------------------------------------------------------------------------------
//  Callback functions for WINMM Wrapper
//-------------------------------------------------------------------------------------------------

type CallbackFunction = unsafe extern "C" fn(HMIDIOUT, DWORD, DWORD_PTR, DWORD_PTR, DWORD_PTR);
unsafe extern "C" fn def_callback(_: HMIDIOUT, _: DWORD, _: DWORD_PTR, _: DWORD_PTR, _: DWORD_PTR)
{
}
static mut DUMMY_DEVICE: HMIDI = std::ptr::null_mut();
static mut CALLBACK_INSTANCE: DWORD_PTR = 0;
static mut CALLBACK: CallbackFunction = def_callback;
static mut CALLBACK_TYPE: DWORD = 0;

#[no_mangle]
pub extern "C" fn ReturnKDMAPIVer(
  Major: *mut c_ulong,
  Minor: *mut c_ulong,
  Build: *mut c_ulong,
  Revision: *mut c_ulong,
) -> u32
{
  println!("ReturnKDMAPIVer");
  unsafe {
    *Major = 4;
    *Minor = 1;
    *Build = 0;
    *Revision = 5;
  }
  1
}

#[no_mangle]
pub extern "C" fn timeGetTime64() -> u64
{
  std::time::SystemTime::now()
    .duration_since(std::time::SystemTime::UNIX_EPOCH)
    .unwrap()
    .as_millis() as u64
}

#[no_mangle]
pub extern "C" fn modMessage() -> u32
{
  println!("modMessage");
  1
}

#[no_mangle]
pub unsafe extern "C" fn InitializeCallbackFeatures(
  OMHM: HMIDI,
  OMCB: CallbackFunction,
  OMI: DWORD_PTR,
  OMU: DWORD_PTR,
  OMCM: DWORD,
) -> u32
{
  println!("InitializeCallbackFeatures");

  DUMMY_DEVICE = OMHM;
  CALLBACK = OMCB;
  CALLBACK_INSTANCE = OMI;
  CALLBACK_TYPE = OMCM;

  if OMCM == CALLBACK_WINDOW
  {
    if CALLBACK != def_callback && IsWindow(CALLBACK as HWND) != 0
    {
      return 0;
    }
  }

  1
}

#[no_mangle]
pub unsafe extern "C" fn RunCallbackFunction(Msg: DWORD, P1: DWORD_PTR, P2: DWORD_PTR)
{
  println!("RunCallbackFunction");

  //We do a match case just to support stuff if needed
  match CALLBACK_TYPE
  {
    CALLBACK_FUNCTION =>
    {
      CALLBACK(DUMMY_DEVICE as HMIDIOUT, Msg, P1, P2, CALLBACK_INSTANCE);
    }
    CALLBACK_EVENT =>
    {
      SetEvent(CALLBACK as HANDLE);
    }
    CALLBACK_THREAD =>
    {
      PostThreadMessageW(CALLBACK as DWORD, Msg, P1, P2.try_into().unwrap());
    }
    CALLBACK_WINDOW =>
    {
      PostMessageW(CALLBACK as HWND, Msg, P1, P2.try_into().unwrap());
    }
    _ => println!("Type was NULL, Do Nothing"),
  }
}
