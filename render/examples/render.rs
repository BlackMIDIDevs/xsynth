use std::{sync::Arc, time::Instant};

use core::{
    channel::{ChannelAudioEvent, ChannelConfigEvent, ControlEvent},
    channel_group::SynthEvent,
    soundfont::{SampleSoundfont, SoundfontBase},
};

use midi_toolkit::{
    events::{Event, MIDIEventEnum},
    io::MIDIFile,
    pipe,
    sequence::{
        event::{cancel_tempo_events, scale_event_time},
        unwrap_items, TimeCaster,
    },
};
use xsynth_render::XSynthRender;

fn main() {
    let mut synth = XSynthRender::new(Default::default(), "out.wav".into());

    println!("Loading Soundfont");

    let soundfonts: Vec<Arc<dyn SoundfontBase>> = vec![Arc::new(
        SampleSoundfont::new(
            "/home/jim/Black MIDIs/SoundFonts/MBMS Soundfonts/CFaz Keys IV Concert Grand Piano/.PianoSamples/cfaz.sfz",
            synth.get_params(),
        )
        .unwrap(),
    )];

    synth.send_event(SynthEvent::ChannelConfig(
        ChannelConfigEvent::SetSoundfonts(soundfonts),
    ));

    println!("Loading MIDI");

    let midi = MIDIFile::open(
        "/home/jim/Black MIDIs/MIDI Files/Danidanijr/4448_U3_Fix.mid",
        None,
    )
    .unwrap();

    let ppq = midi.ppq();
    let merged = pipe!(
        midi.iter_all_track_events_merged_batches()
        |>TimeCaster::<f64>::cast_event_delta()
        |>cancel_tempo_events(250000)
        |>scale_event_time(1.0 / ppq as f64)
        |>unwrap_items()
    );

    let collected = merged.collect::<Vec<_>>();

    let render_time = Instant::now();

    println!("Starting rendering");

    for batch in collected.into_iter() {
        if batch.delta > 0.0 {
            synth.render_batch(batch.delta);
        }
        for e in batch.iter_events() {
            match e.as_event() {
                Event::NoteOn(e) => {
                    synth.send_event(SynthEvent::Channel(
                        e.channel as u32,
                        ChannelAudioEvent::NoteOn {
                            key: e.key,
                            vel: e.velocity,
                        },
                    ));
                }
                Event::NoteOff(e) => {
                    synth.send_event(SynthEvent::Channel(
                        e.channel as u32,
                        ChannelAudioEvent::NoteOff { key: e.key },
                    ));
                }
                Event::ControlChange(e) => {
                    synth.send_event(SynthEvent::Channel(
                        e.channel as u32,
                        ChannelAudioEvent::Control(ControlEvent::Raw(e.controller, e.value)),
                    ));
                }
                Event::PitchWheelChange(e) => {
                    synth.send_event(SynthEvent::Channel(
                        e.channel as u32,
                        ChannelAudioEvent::Control(ControlEvent::PitchBendValue(
                            e.pitch as f32 / 8192.0,
                        )),
                    ));
                }
                _ => {}
            }
        }
    }

    synth.render_batch(3.0);

    println!("Render finished");
    println!("Render time: {} seconds", render_time.elapsed().as_secs());
    synth.finalize();
}
