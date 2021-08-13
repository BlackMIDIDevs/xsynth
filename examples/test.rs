use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use xsynth::{
    RealtimeSynth,
};

fn main() {
    let host = cpal::default_host();

    let device = host
        .default_output_device()
        .expect("failed to find output device");
    println!("Output device: {}", device.name().unwrap());

    let config = device.default_output_config().unwrap();
    println!("Default output config: {:?}", config);

    let synth = RealtimeSynth::new(16, &device, config);

    loop {
        for i in 0..128 {
            for chan in 0..1 {
                for _ in 0..20 {
                    // synth.send_event(SynthEvent::new(
                    //     chan,
                    //     ChannelEvent::NoteOff { key: i as u8 },
                    // ));
                    // synth.send_event(SynthEvent::new(
                    //     chan,
                    //     ChannelEvent::NoteOn {
                    //         key: i as u8,
                    //         vel: 64,
                    //     },
                    // ));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    std::thread::sleep(Duration::from_secs(10000));
}
