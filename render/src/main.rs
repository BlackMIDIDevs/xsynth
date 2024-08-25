mod config;
use config::*;

mod rendered;
use rendered::*;

mod utils;
use utils::get_midi_length;

mod writer;

use xsynth_core::{
    channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent, ControlEvent},
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

use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use atomic_float::AtomicF64;

fn main() {
    let state = State::from_args();

    let mut synth = XSynthRender::new(state.config.clone(), state.output.clone());

    print!("Loading soundfonts...");
    synth.send_event(SynthEvent::AllChannels(ChannelEvent::Config(
        ChannelConfigEvent::SetSoundfonts(
            state
                .soundfonts
                .iter()
                .map(|s| {
                    let sf: Arc<dyn SoundfontBase> = Arc::new(
                        SampleSoundfont::new(s, synth.get_params(), state.config.sf_options)
                            .unwrap(),
                    );
                    sf
                })
                .collect::<Vec<Arc<dyn SoundfontBase>>>(),
        ),
    )));

    synth.send_event(SynthEvent::AllChannels(ChannelEvent::Config(
        ChannelConfigEvent::SetLayerCount(state.layers),
    )));

    let length = get_midi_length(state.midi.to_str().unwrap());

    let midi = MIDIFile::open(state.midi, None).unwrap();

    let ppq = midi.ppq();
    let merged = pipe!(
        midi.iter_all_track_events_merged_batches()
        |>TimeCaster::<f64>::cast_event_delta()
        |>cancel_tempo_events(250000)
        |>scale_event_time(1.0 / ppq as f64)
        |>unwrap_items()
    );

    let (snd, rcv) = crossbeam_channel::bounded(100);

    thread::spawn(move || {
        for batch in merged {
            snd.send(batch).unwrap();
        }
    });

    let position = Arc::new(AtomicF64::new(0.0));
    let voices = Arc::new(AtomicU64::new(0));

    {
        let position = position.clone();
        let voices = voices.clone();

        thread::spawn(move || loop {
            let pos = position.load(Ordering::Relaxed);
            let progress = (pos / length) * 100.0 + 0.0004;
            print!("\rProgress: [");
            let bars = progress as u8 / 5;
            for _ in 0..bars {
                print!("=");
            }
            for _ in 0..(20 - bars) {
                print!(" ");
            }
            print!("] {progress:.3}% | ");
            print!("Voice Count: {}", voices.load(Ordering::Relaxed));
            for _ in 0..10 {
                print!(" ");
            }
            if progress >= 100.0 {
                println!();
                break;
            }
        });
    }

    let now = Instant::now();

    for batch in rcv {
        if batch.delta > 0.0 {
            synth.render_batch(batch.delta);
            position.fetch_add(batch.delta, Ordering::Relaxed);
            voices.store(synth.voice_count(), Ordering::Relaxed);
        }
        for e in batch.iter_events() {
            match e.as_event() {
                Event::NoteOn(e) => {
                    synth.send_event(SynthEvent::Channel(
                        e.channel as u32,
                        ChannelEvent::Audio(ChannelAudioEvent::NoteOn {
                            key: e.key,
                            vel: e.velocity,
                        }),
                    ));
                }
                Event::NoteOff(e) => {
                    synth.send_event(SynthEvent::Channel(
                        e.channel as u32,
                        ChannelEvent::Audio(ChannelAudioEvent::NoteOff { key: e.key }),
                    ));
                }
                Event::ControlChange(e) => {
                    synth.send_event(SynthEvent::Channel(
                        e.channel as u32,
                        ChannelEvent::Audio(ChannelAudioEvent::Control(ControlEvent::Raw(
                            e.controller,
                            e.value,
                        ))),
                    ));
                }
                Event::PitchWheelChange(e) => {
                    synth.send_event(SynthEvent::Channel(
                        e.channel as u32,
                        ChannelEvent::Audio(ChannelAudioEvent::Control(
                            ControlEvent::PitchBendValue(e.pitch as f32 / 8192.0),
                        )),
                    ));
                }
                Event::ProgramChange(e) => {
                    synth.send_event(SynthEvent::Channel(
                        e.channel as u32,
                        ChannelEvent::Audio(ChannelAudioEvent::ProgramChange(e.program)),
                    ));
                }
                _ => {}
            }
        }
    }
    synth.send_event(SynthEvent::AllChannels(ChannelEvent::Audio(
        ChannelAudioEvent::AllNotesOff,
    )));
    synth.send_event(SynthEvent::AllChannels(ChannelEvent::Audio(
        ChannelAudioEvent::ResetControl,
    )));
    synth.finalize();

    let elapsed = now.elapsed();
    thread::sleep(Duration::from_millis(200));
    println!("Render time: {:?}", elapsed);
}
