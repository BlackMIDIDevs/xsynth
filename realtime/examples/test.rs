use std::{sync::Arc, time::Duration};
use xsynth_core::{
    channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent},
    soundfont::{SampleSoundfont, SoundfontBase},
};

use xsynth_realtime::{RealtimeSynth, SynthEvent};

fn main() {
    let args = std::env::args().collect::<Vec<String>>();
    let Some(sfz) = args
        .get(1)
        .cloned()
        .or_else(|| std::env::var("XSYNTH_EXAMPLE_SFZ").ok())
    else {
        println!(
            "Usage: {} [sfz]",
            std::env::current_exe()
                .unwrap_or("example".into())
                .display()
        );
        return;
    };

    let synth = RealtimeSynth::open_with_all_defaults();
    let mut sender = synth.get_senders();

    let params = synth.stream_params();

    let soundfonts: Vec<Arc<dyn SoundfontBase>> = vec![Arc::new(
        SampleSoundfont::new(sfz, params, Default::default()).unwrap(),
    )];

    sender.send_event(SynthEvent::AllChannels(ChannelEvent::Config(
        ChannelConfigEvent::SetSoundfonts(soundfonts),
    )));

    // for k in 0..127 {
    //     for c in 0..16 {
    //         for _ in 0..16 {
    //             synth.send_event(SynthEvent::Channel(
    //                 c,
    //                 ChannelEvent::NoteOn { key: k, vel: 5 },
    //             ));
    //         }
    //     }
    // }
    sender.send_event(SynthEvent::Channel(
        0,
        ChannelEvent::Audio(ChannelAudioEvent::NoteOn { key: 10, vel: 127 }),
    ));

    std::thread::sleep(Duration::from_secs(10000));
}
