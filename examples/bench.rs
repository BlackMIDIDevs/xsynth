use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait};
use xsynth::{core::event::ChannelEvent, RealtimeSynth, SynthEvent};

fn main() {
    let host = cpal::default_host();

    let device = host
        .default_output_device()
        .expect("failed to find output device");
    println!("Output device: {}", device.name().unwrap());

    let config = device.default_output_config().unwrap();
    println!("Default output config: {:?}", config);
    let elapsed = {
        let mut synth = RealtimeSynth::new(16, &device, config);

        let start = Instant::now();
        for _ in 0..100000 {
            for _ in 0..100 {
                synth.send_event(SynthEvent::Channel(
                    0,
                    ChannelEvent::NoteOn { key: 0, vel: 5 },
                ));
            }
            for _ in 0..100 {
                synth.send_event(SynthEvent::Channel(0, ChannelEvent::NoteOff { key: 0 }));
            }
        }
        start.elapsed()
    };

    std::thread::sleep(Duration::from_secs(2));

    dbg!(elapsed);
}
