use std::{sync::Arc, time::Instant};

use xsynth_core::{
    channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent, VoiceChannel},
    soundfont::{Interpolator, SampleSoundfont, SoundfontBase, SoundfontInitOptions},
    AudioPipe, AudioStreamParams, ChannelCount,
};

pub fn main() {
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

    let stream_params = AudioStreamParams::new(48000, ChannelCount::Stereo);

    println!("Loading soundfont...");

    let soundfonts: Vec<Arc<dyn SoundfontBase>> = vec![Arc::new(
        SampleSoundfont::new(
            sfz,
            stream_params,
            SoundfontInitOptions {
                bank: None,
                preset: None,
                interpolator: Interpolator::Nearest,
                linear_release: false,
                use_effects: false,
            },
        )
        .unwrap(),
    )];

    println!("Initializing channel");

    let layer_count = 512 * 4;

    let threadpool = rayon::ThreadPoolBuilder::new().build().unwrap();

    let mut channel = VoiceChannel::new(
        Default::default(),
        stream_params,
        Some(Arc::new(threadpool)),
    );
    channel.process_event(ChannelEvent::Config(ChannelConfigEvent::SetSoundfonts(
        soundfonts.clone(),
    )));
    channel.process_event(ChannelEvent::Config(ChannelConfigEvent::SetLayerCount(
        Some(layer_count as usize),
    )));

    for _ in 0..layer_count {
        for i in 0..127 {
            channel.process_event(ChannelEvent::Audio(ChannelAudioEvent::NoteOn {
                key: i as u8,
                vel: 127,
            }));
        }
    }

    let mut buffer = vec![0.0; 4800];
    channel.read_samples(&mut buffer);

    println!("Running bench with {} layers", layer_count);
    println!("Voice count: {}", channel.get_channel_stats().voice_count());

    let now = Instant::now();
    let loops = 10;
    for _ in 0..loops {
        channel.read_samples(&mut buffer);
    }

    println!("Render time: {} seconds", now.elapsed().as_secs_f64());

    // Calculate samples per second
    let samples_rendered = layer_count * 127 * buffer.len() as u64 * loops * 2;
    let seconds = now.elapsed().as_secs_f64();
    let samples_per_second = samples_rendered as f64 / seconds;

    println!("Samples per second: {}", samples_per_second);
}
