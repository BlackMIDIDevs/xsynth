use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use xsynth::{
    core::{event::ChannelEvent, AudioPipe, BufferedRenderer, FunctionAudioPipe, VoiceChannel},
    RealtimeSynth, SynthEvent,
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

    for i in 0..128 {
        for _ in 0..64 {
            synth.send_event(SynthEvent::new(
                0,
                ChannelEvent::NoteOn {
                    key: i as u8,
                    vel: 64,
                },
            ));
        }
    }

    std::thread::sleep(Duration::from_secs(10000));
}
