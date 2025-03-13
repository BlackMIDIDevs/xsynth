use std::error::Error;
use std::io::stdin;

use hotwatch::{Event, EventKind, Hotwatch};
use midir::os::unix::VirtualInput;
use midir::{Ignore, MidiInput};
use std::{thread, time::Duration};
use xsynth_core::channel::{ChannelConfigEvent, ChannelEvent};
use xsynth_realtime::{RealtimeSynth, SynthEvent};

mod parsers;
use parsers::*;

fn main() {
    match run() {
        Ok(_) => (),
        Err(err) => println!("Error: {}", err),
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let config = Config::<Settings>::new().load().unwrap();
    let sflist = Config::<SFList>::new().load().unwrap();

    let realtime_synth = RealtimeSynth::open_with_default_output(config.get_synth_config());
    let mut sender = realtime_synth.get_sender_ref().clone();
    let params = realtime_synth.stream_params();

    sender.send_event(SynthEvent::AllChannels(ChannelEvent::Config(
        ChannelConfigEvent::SetLayerCount(config.get_layers()),
    )));
    sender.send_event(SynthEvent::AllChannels(ChannelEvent::Config(
        ChannelConfigEvent::SetSoundfonts(sflist.create_sfbase_vector(params)),
    )));

    let mut midi_in = MidiInput::new("XSynth")?;
    midi_in.ignore(Ignore::None);

    // _conn_in needs to be a named parameter, because it needs to be kept alive until the end of the scope
    let mut sender_thread = sender.clone();
    let _conn_in = midi_in.create_virtual(
        "XSynth MIDI In",
        move |_, message, _| {
            //println!("{}: {:?} (len = {})", stamp, message, message.len());
            //Send the MIDI message to the synth as raw bytes
            sender_thread.send_event_u32(
                message.get(0).copied().unwrap_or(0) as u32
                    | (message.get(1).copied().unwrap_or(0) as u32) << 8
                    | (message.get(2).copied().unwrap_or(0) as u32) << 16,
            );
        },
        (),
    )?;

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

    println!("Press any key to exit...");
    let mut input = String::new();
    stdin().read_line(&mut input)?;
    println!("Shutting down...");
    Ok(())
}
