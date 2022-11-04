use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use core::{
    channel::{ChannelAudioEvent, ChannelConfigEvent, ControlEvent},
    soundfont::{SampleSoundfont, SoundfontBase},
    channel_group::SynthEvent,
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
            "/home/jim/Projects/SoundFonts/AIG/Preset-1L.sfz",
            synth.get_params(),
        )
        .unwrap(),
    )];

    synth.send_event(SynthEvent::ChannelConfig(ChannelConfigEvent::SetSoundfonts(soundfonts)));

    println!("Loading MIDI");

    let midi =
    MIDIFile::open("/home/jim/Black MIDIs/MIDI Files/Infernis/Impossible Piano - HSiFS - Crazy Backup Dancers ][ black.mid", None).unwrap();

    let ppq = midi.ppq();
    let merged = pipe!(
        midi.iter_all_track_events_merged_batches()
        |>TimeCaster::<f64>::cast_event_delta()
        |>cancel_tempo_events(250000)
        |>scale_event_time(1.0 / ppq as f64)
        |>unwrap_items()
    );

    let collected = merged.collect::<Vec<_>>();

    let now = Instant::now() - Duration::from_secs_f64(0.0);
    let mut time = 0.0;

    let render_time = Instant::now();

    println!("Starting rendering");

    for batch in collected.into_iter() {
        if batch.delta > 0.0 {
            time += batch.delta;
            synth.render_batch(batch.delta);

            let diff = time - now.elapsed().as_secs_f64();
            if diff > 0.0 {
                spin_sleep::sleep(Duration::from_secs_f64(diff));
            }
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

    println!("Render finished");
    println!("Render time: {} seconds", render_time.elapsed().as_secs());
    synth.finalize();
}
