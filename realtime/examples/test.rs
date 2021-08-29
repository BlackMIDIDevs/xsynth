use std::time::Duration;

use core::event::ChannelEvent;
use cpal::traits::{DeviceTrait, HostTrait};
use xsynth_realtime::{RealtimeSynth, SynthEvent};

fn main() {
    let host = cpal::default_host();

    let device = host
        .default_output_device()
        .expect("failed to find output device");
    println!("Output device: {}", device.name().unwrap());

    let config = device.default_output_config().unwrap();
    println!("Default output config: {:?}", config);
    let mut synth = RealtimeSynth::new(16, &device, config);

    for k in 0..127 {
        for c in 0..16 {
            for _ in 0..16 {
                synth.send_event(SynthEvent::Channel(
                    c,
                    ChannelEvent::NoteOn { key: k, vel: 5 },
                ));
            }
        }
    }

    std::thread::sleep(Duration::from_secs(10000));
}
