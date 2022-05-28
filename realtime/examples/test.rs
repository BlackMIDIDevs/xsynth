use core::{
    channel::ChannelEvent,
    soundfont::{SoundfontBase, SquareSoundfont},
};
use std::{sync::Arc, time::Duration};

use xsynth_realtime::{RealtimeSynth, SynthEvent};

fn main() {
    let synth = RealtimeSynth::open_with_all_defaults();
    let mut sender = synth.get_senders();

    let params = synth.stream_params();

    let soundfonts: Vec<Arc<dyn SoundfontBase>> = vec![Arc::new(SquareSoundfont::new(
        params.sample_rate,
        params.channels,
    ))];

    sender.send_event(SynthEvent::AllChannels(ChannelEvent::SetSoundfonts(
        soundfonts,
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
        ChannelEvent::NoteOn { key: 10, vel: 127 },
    ));

    std::thread::sleep(Duration::from_secs(10000));
}
