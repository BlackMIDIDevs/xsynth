use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use core::{
    channel::{ChannelAudioEvent, ChannelConfigEvent, ControlEvent},
    soundfont::{SampleSoundfont, SoundfontBase},
    channel_group::SynthEvent,
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
use xsynth_render::{XSynthRender, config::XSynthRenderConfig};

fn main() {
    let mut synth = XSynthRender::new(Default::default(), "out.wav".into());

    let soundfonts: Vec<Arc<dyn SoundfontBase>> = vec![Arc::new(
        SampleSoundfont::new(
            "/home/jim/Projects/SoundFonts/AIG/Preset-1L.sfz",
            synth.get_params(),
        )
        .unwrap(),
    )];

    synth.send_event(SynthEvent::ChannelConfig(ChannelConfigEvent::SetSoundfonts(soundfonts)));

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

    //synth.start_render();

    let now = Instant::now() - Duration::from_secs_f64(0.0);
    let mut time = 0.0;
    for e in collected.into_iter() {
        if e.delta() != 0.0 {
            time += e.delta();
            let diff = time - now.elapsed().as_secs_f64();
            if diff > 0.0 {
                synth.render_batch(diff);
                spin_sleep::sleep(Duration::from_secs_f64(diff));
            }
        }

        match e {
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

    //synth.finalize();
    //std::thread::sleep(Duration::from_secs(10000));
}
