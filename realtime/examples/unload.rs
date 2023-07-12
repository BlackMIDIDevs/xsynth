use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use core::{
    channel::{ChannelAudioEvent, ChannelConfigEvent, ControlEvent},
    soundfont::{SampleSoundfont, SoundfontBase},
};
use midi_toolkit::{
    events::Event,
    io::MIDIFile,
    pipe,
    sequence::{
        event::{cancel_tempo_events, scale_event_time},
        unwrap_items, TimeCaster,
    },
};
use xsynth_realtime::{RealtimeSynth, SynthEvent};

fn main() {
    let synth = RealtimeSynth::open_with_all_defaults();
    let mut sender = synth.get_senders();

    let params = synth.stream_params();

    let args = std::env::args().collect::<Vec<String>>();
    let (Some(sfz),) = (
        args.get(1)
            .cloned()
            .or_else(|| std::env::var("XSYNTH_EXAMPLE_SFZ").ok()),
    ) else {
        println!(
            "Usage: {} [sfz]",
            std::env::current_exe()
                .unwrap_or("example".into())
                .display()
        );
        return;
    };

    println!("Loading Soundfont");
    let soundfonts: Vec<Arc<dyn SoundfontBase>> = vec![Arc::new(
        SampleSoundfont::new(sfz, params, Default::default()).unwrap(),
    )];
    println!("Loaded");

    sender.send_config(ChannelConfigEvent::SetSoundfonts(soundfonts));

    sender.send_event(SynthEvent::Channel(
        0,
        ChannelAudioEvent::NoteOn { key: 64, vel: 127 },
    ));

    std::thread::sleep(Duration::from_secs(1));

    println!("unloading");
    drop(sender);
    drop(synth);
    println!("unloaded");

    std::thread::sleep(Duration::from_secs(10));
}
