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
    events::{Event, MIDIEvent},
    io::MIDIFile,
    pipe,
    sequence::{
        event::{cancel_tempo_events, scale_event_time},
        unwrap_items, TimeCaster,
    },
};
use xsynth_realtime::{RealtimeSynth, SynthEvent};
use hound;

fn main() {
    let synth = RealtimeSynth::open_with_all_defaults();
    let mut sender = synth.get_senders();

    let params = synth.stream_params();

    let soundfonts: Vec<Arc<dyn SoundfontBase>> = vec![Arc::new(
        SampleSoundfont::new(
            "/home/jim/Projects/SoundFonts/AIG/Preset-1L.sfz",
            params.clone(),
        )
        .unwrap(),
    )];

    sender.send_config(ChannelConfigEvent::SetSoundfonts(soundfonts));

    let midi =
        MIDIFile::open("/home/jim/Black MIDIs/MIDI Files/Infernis/Impossible Piano - HSiFS - Crazy Backup Dancers ][ black.mid", None).unwrap();

    let ppq = midi.ppq();
    let merged = pipe!(
        midi.iter_all_events_merged()
        |>TimeCaster::<f64>::cast_event_delta()
        |>cancel_tempo_events(250000)
        |>scale_event_time(1.0 / ppq as f64)
        |>unwrap_items()
    );

    let collected = merged.collect::<Vec<_>>();

    let stats = synth.get_stats();
    thread::spawn(move || loop {
        print!(
            "Voice Count: {}  \tBuffer: {}\tRender time: {}\r",
            stats.voice_count(),
            stats.buffer().last_samples_after_read(),
            stats.buffer().average_renderer_load()
        );
        thread::sleep(Duration::from_millis(10));
    });

    let buffer = synth.read_buffer();
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 44100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create("out.wav", spec).unwrap();

    let mut has_started = false;

    thread::spawn(move || loop {
        for s in &buffer {
            if !has_started {
                if s != 0.0 {
                    has_started = true;
                }
            } else {
                writer.write_sample((s*32768f32).round() as i16).unwrap();
            }
        }
    });

    let now = Instant::now() - Duration::from_secs_f64(0.0);
    let mut time = 0.0;
    for e in collected.into_iter() {
        if e.delta() != 0.0 {
            time += e.delta();
            let diff = time - now.elapsed().as_secs_f64();
            if diff > 0.0 {
                spin_sleep::sleep(Duration::from_secs_f64(diff));
            }
        }

        match e {
            Event::NoteOn(e) => {
                sender.send_event(SynthEvent::Channel(
                    e.channel as u32,
                    ChannelAudioEvent::NoteOn {
                        key: e.key,
                        vel: e.velocity,
                    },
                ));
            }
            Event::NoteOff(e) => {
                sender.send_event(SynthEvent::Channel(
                    e.channel as u32,
                    ChannelAudioEvent::NoteOff { key: e.key },
                ));
            }
            Event::ControlChange(e) => {
                sender.send_event(SynthEvent::Channel(
                    e.channel as u32,
                    ChannelAudioEvent::Control(ControlEvent::Raw(e.controller, e.value)),
                ));
            }
            Event::PitchWheelChange(e) => {
                sender.send_event(SynthEvent::Channel(
                    e.channel as u32,
                    ChannelAudioEvent::Control(ControlEvent::PitchBendValue(
                        e.pitch as f32 / 8192.0,
                    )),
                ));
            }
            _ => {}
        }
    }

    std::thread::sleep(Duration::from_secs(10000));
}
