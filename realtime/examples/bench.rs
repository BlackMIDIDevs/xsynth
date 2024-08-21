use std::time::{Duration, Instant};
use xsynth_core::channel::{ChannelAudioEvent, ChannelEvent};

use xsynth_realtime::{RealtimeSynth, SynthEvent};

fn main() {
    let elapsed = {
        let mut synth = RealtimeSynth::open_with_all_defaults();

        let start = Instant::now();
        for _ in 0..100000 {
            for _ in 0..100 {
                synth.send_event(SynthEvent::Channel(
                    0,
                    ChannelEvent::Audio(ChannelAudioEvent::NoteOn { key: 0, vel: 5 }),
                ));
            }
            for _ in 0..100 {
                synth.send_event(SynthEvent::Channel(
                    0,
                    ChannelEvent::Audio(ChannelAudioEvent::NoteOff { key: 0 }),
                ));
            }
        }
        start.elapsed()
    };

    std::thread::sleep(Duration::from_secs(2));

    dbg!(elapsed);
}
