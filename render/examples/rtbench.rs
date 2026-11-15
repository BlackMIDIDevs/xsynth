//! Realtime-simulation benchmark.
//!
//! Plays a MIDI through a `ChannelGroup` in fixed-size buffers, feeding the
//! events of each buffer before rendering it (like a realtime callback), and
//! measures every render against the buffer's realtime deadline.
//!
//! Usage: rtbench <midi> <soundfont> [--buffer FRAMES] [--rate HZ]
//!        [--start SEC] [--seconds SEC] [--layers N(0=unlimited)]
//!        [--threads N] [--per-channel]
//!
//! Audio before `--start` is rendered with large buffers and not measured.

use std::{sync::Arc, time::Instant};

use midi_toolkit::{
    events::{Event, MIDIEventEnum},
    io::MIDIFile,
    prelude::*,
};
use xsynth_core::{
    channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent, ControlEvent},
    channel_group::{
        ChannelGroup, ChannelGroupConfig, ParallelismOptions, SynthEvent, SynthFormat, ThreadCount,
    },
    soundfont::{SampleSoundfont, SoundfontBase},
    AudioPipe, AudioStreamParams, ChannelCount,
};

fn arg<T: std::str::FromStr>(args: &[String], name: &str, default: T) -> T {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn to_synth_event(e: &Event) -> Option<SynthEvent> {
    let (ch, ev) = match e {
        Event::NoteOn(e) => (
            e.channel,
            ChannelAudioEvent::NoteOn {
                key: e.key,
                vel: e.velocity,
            },
        ),
        Event::NoteOff(e) => (e.channel, ChannelAudioEvent::NoteOff { key: e.key }),
        Event::ControlChange(e) => (
            e.channel,
            ChannelAudioEvent::Control(ControlEvent::Raw(e.controller, e.value)),
        ),
        Event::PitchWheelChange(e) => (
            e.channel,
            ChannelAudioEvent::Control(ControlEvent::PitchBendValue(e.pitch as f32 / 8192.0)),
        ),
        Event::ProgramChange(e) => (e.channel, ChannelAudioEvent::ProgramChange(e.program)),
        _ => return None,
    };
    Some(SynthEvent::Channel(ch as u32, ChannelEvent::Audio(ev)))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        println!("Usage: rtbench <midi> <soundfont> [--buffer FRAMES] [--rate HZ] [--start SEC] [--seconds SEC] [--layers N] [--threads N] [--per-channel]");
        return;
    }
    let buffer_frames: usize = arg(&args, "--buffer", 64);
    let rate: u32 = arg(&args, "--rate", 48000);
    let start: f64 = arg(&args, "--start", 0.0);
    let seconds: f64 = arg(&args, "--seconds", 30.0);
    let layers: usize = arg(&args, "--layers", 0);
    let threads: usize = arg(&args, "--threads", 0);
    let per_channel = args.iter().any(|a| a == "--per-channel");

    let params = AudioStreamParams::new(rate, ChannelCount::Stereo);
    let t = Instant::now();
    let sf: Arc<dyn SoundfontBase> =
        Arc::new(SampleSoundfont::new(&args[2], params, Default::default()).unwrap());
    println!("Soundfont loaded in {:.2?}", t.elapsed());

    let tc = if threads == 0 {
        ThreadCount::Auto
    } else {
        ThreadCount::Manual(threads)
    };
    let mut synth = ChannelGroup::new(ChannelGroupConfig {
        channel_init_options: Default::default(),
        format: SynthFormat::Midi,
        audio_params: params,
        parallelism: ParallelismOptions {
            channel: tc,
            key: if per_channel { ThreadCount::None } else { tc },
        },
    });
    synth.send_event(SynthEvent::AllChannels(ChannelEvent::Config(
        ChannelConfigEvent::SetSoundfonts(vec![sf]),
    )));
    synth.send_event(SynthEvent::AllChannels(ChannelEvent::Config(
        ChannelConfigEvent::SetLayerCount(if layers == 0 { None } else { Some(layers) }),
    )));

    let midi = MIDIFile::open(&args[1], None).unwrap();
    let ppq = midi.ppq();
    let merged = midi
        .iter_all_track_events_merged_batches()
        .cast_event_delta::<f64>()
        .cancel_tempo_events(250000)
        .scale_event_time(1.0 / ppq as f64)
        .unwrap_items();

    // Parse ahead on another thread so MIDI decoding is not measured.
    let (snd, rcv) = crossbeam_channel::bounded(1 << 16);
    std::thread::spawn(move || {
        let mut time = 0.0;
        for batch in merged {
            time += batch.delta;
            let events: Vec<SynthEvent> = batch
                .iter_events()
                .filter_map(|e| to_synth_event(e.as_event()))
                .collect();
            if snd.send((time, events)).is_err() {
                break;
            }
        }
    });

    let deadline = buffer_frames as f64 / rate as f64;
    let mut buf = vec![0.0f32; buffer_frames * 2];
    let mut warm_buf = vec![0.0f32; 4800 * 2];
    let mut pending: Option<(f64, Vec<SynthEvent>)> = None;
    let mut now = 0.0f64;
    let mut feed = |synth: &mut ChannelGroup, until: f64| -> bool {
        loop {
            let item = match pending.take() {
                Some(p) => p,
                None => match rcv.recv() {
                    Ok(p) => p,
                    Err(_) => return false,
                },
            };
            if item.0 >= until {
                pending = Some(item);
                return true;
            }
            for e in item.1 {
                synth.send_event(e);
            }
        }
    };

    // Warm-up: render up to `start` without measuring.
    let t = Instant::now();
    while now + 0.1 <= start {
        feed(&mut synth, now + 0.1);
        synth.read_samples(&mut warm_buf);
        now += 0.1;
    }
    if start > 0.0 {
        println!(
            "Skipped to {:.1}s in {:.2?} ({} voices)",
            now,
            t.elapsed(),
            synth.voice_count()
        );
    }

    let mut times = Vec::new();
    let mut voices = Vec::new();
    let mut checksum = 0.0f64;
    let total_buffers = (seconds / deadline) as usize;
    let wall = Instant::now();
    let progress_every = (1.0 / deadline) as usize;
    for i in 0..total_buffers {
        if i % progress_every == 0 && std::env::var_os("RTBENCH_PROGRESS").is_some() {
            eprintln!(
                "t={:.1}s voices={} wall={:.1}s",
                now,
                synth.voice_count(),
                wall.elapsed().as_secs_f64()
            );
        }
        now += deadline;
        let more = feed(&mut synth, now);
        let t = Instant::now();
        synth.read_samples(&mut buf);
        times.push(t.elapsed().as_secs_f64());
        voices.push(synth.voice_count());
        checksum += buf.iter().map(|s| s.abs() as f64).sum::<f64>();
        if !more && synth.voice_count() == 0 {
            break;
        }
    }
    let wall = wall.elapsed().as_secs_f64();
    let audio = times.len() as f64 * deadline;
    let render_total: f64 = times.iter().sum();

    let mut loads: Vec<f64> = times.iter().map(|t| t / deadline).collect();
    let max_voices = *voices.iter().max().unwrap_or(&0);
    let avg_voices = voices.iter().sum::<u64>() as f64 / voices.len().max(1) as f64;
    let overruns = loads.iter().filter(|l| **l > 1.0).count();
    // voices * frames per render second
    let voice_frames: f64 = voices.iter().map(|v| *v as f64).sum::<f64>() * buffer_frames as f64;
    loads.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pct = |p: f64| loads[((loads.len() - 1) as f64 * p) as usize];

    println!(
        "buffer={} frames ({:.2} ms) rate={} layers={} threads={} mode={}",
        buffer_frames,
        deadline * 1000.0,
        rate,
        if layers == 0 {
            "unlimited".to_string()
        } else {
            layers.to_string()
        },
        if threads == 0 {
            "auto".to_string()
        } else {
            threads.to_string()
        },
        if per_channel {
            "per-channel"
        } else {
            "per-key"
        }
    );
    println!(
        "audio {:.2}s rendered in {:.2}s (render {:.2}s) => {:.2}x realtime",
        audio,
        wall,
        render_total,
        audio / render_total
    );
    println!("voices: avg {:.0} max {}", avg_voices, max_voices);
    println!(
        "load (render/deadline): p50 {:.3} p90 {:.3} p99 {:.3} p99.9 {:.3} max {:.3}",
        pct(0.5),
        pct(0.9),
        pct(0.99),
        pct(0.999),
        pct(1.0)
    );
    println!(
        "overruns: {} / {} buffers ({:.3}%)",
        overruns,
        loads.len(),
        overruns as f64 * 100.0 / loads.len() as f64
    );
    println!(
        "throughput: {:.1} M voice-frames/s",
        voice_frames / render_total / 1e6
    );
    println!("checksum: {:.6e}", checksum);
}
