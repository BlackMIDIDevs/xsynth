pub mod config;
pub use config::*;

mod rendered;
pub use rendered::*;

mod writer;

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

/// Will convert a MIDI to an audio file using the specified soundfont
/// and will return the time it took to render in seconds.
pub fn render_to_file(
    config: XSynthRenderConfig,
    midi_path: &str,
    sfz_paths: Vec<&str>,
    out_path: &str,
) -> u64 {
    let mut synth = XSynthRender::new(config, out_path.into());

    let mut soundfonts: Vec<Arc<dyn SoundfontBase>> = vec![];

    for sfz in sfz_paths {
        soundfonts.push(Arc::new(
            SampleSoundfont::new(sfz, synth.get_params()).unwrap(),
        ));
    }

    synth.send_event(SynthEvent::ChannelConfig(
        ChannelConfigEvent::SetSoundfonts(soundfonts),
    ));

    let midi = MIDIFile::open(midi_path, None).unwrap();

    let ppq = midi.ppq();
    let merged = pipe!(
        midi.iter_all_track_events_merged_batches()
        |>TimeCaster::<f64>::cast_event_delta()
        |>cancel_tempo_events(250000)
        |>scale_event_time(1.0 / ppq as f64)
        |>unwrap_items()
    );

    let render_time = Instant::now();

    for batch in merged {
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
    synth.finalize();

    render_time.elapsed().as_secs()
}
