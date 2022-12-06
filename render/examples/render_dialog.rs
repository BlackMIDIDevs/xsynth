use xsynth_render::{xsynth_renderer, XSynthRenderStats};

use midi_toolkit::{io::MIDIFile, sequence::event::get_channels_array_statistics};

use std::{
    io,
    io::prelude::*,
    io::Write,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    thread,
    time::Instant,
};

use atomic_float::AtomicF64;

fn main() {
    println!("--- FILE PATHS ---");
    let midi_path = read_input("Enter MIDI path");
    let sfz_path = read_input("Enter SFZ path");
    let out_path = read_input("Enter output path");

    println!("\n--- RENDER OPTIONS ---");
    let sample_rate: u32 = read_input("Enter sample rate (in Hz)").parse().unwrap();
    let use_threadpool = read_input_bool("Use threadpool? (y/n)");
    let use_limiter = read_input_bool("Use audio limiter? (y/n)");

    io::stdout().lock().flush().unwrap();

    println!("\n--- STARTING RENDER ---");

    let render_time = Instant::now();
    let position = Arc::new(AtomicF64::new(0.0));
    let voices = Arc::new(AtomicU64::new(0));

    let callback = |stats: XSynthRenderStats| {
        position.store(stats.progress, Ordering::Relaxed);
        voices.store(stats.voice_count, Ordering::Relaxed);
    };

    let position_thread = position.clone();
    let voices_thread = voices.clone();
    let length = get_midi_length(&midi_path);

    thread::spawn(move || loop {
        let pos = position_thread.load(Ordering::Relaxed);
        let progress = (pos / length) * 100.0 + 0.0004;
        print!("\rProgress: [");
        let bars = progress as u8 / 5;
        for _ in 0..bars {
            print!("=");
        }
        for _ in 0..(20 - bars) {
            print!(" ");
        }
        print!("] {:.3}% | ", progress);
        print!("Voice Count: {}", voices_thread.load(Ordering::Relaxed));
        if progress >= 100.0 {
            break;
        }
    });

    xsynth_renderer(&midi_path, &out_path)
        .with_sample_rate(sample_rate)
        .use_threadpool(use_threadpool)
        .use_limiter(use_limiter)
        .add_soundfont(&sfz_path)
        .with_progress_callback(callback)
        .run();

    println!(
        "\n\n--- RENDER FINISHED ---\nRender time: {} seconds",
        render_time.elapsed().as_secs()
    );
    pause();
}

fn read_input(prompt: &str) -> String {
    let stdout = io::stdout();
    let reader = io::stdin();

    let mut input = String::new();
    print!("{prompt}: ");
    stdout.lock().flush().unwrap();
    reader.read_line(&mut input).unwrap();
    let string = input.trim();

    string.to_string()
}

fn read_input_bool(prompt: &str) -> bool {
    let string = read_input(prompt);
    match &string[..] {
        "y" => true,
        "n" => false,
        _ => read_input_bool(prompt),
    }
}

fn get_midi_length(path: &str) -> f64 {
    let midi = MIDIFile::open(path, None).unwrap();
    let parse_length_outer = Arc::new(AtomicF64::new(f64::NAN));
    let ppq = midi.ppq();
    let tracks = midi.iter_all_tracks().collect();
    let stats = get_channels_array_statistics(tracks);
    if let Ok(stats) = stats {
        parse_length_outer.store(
            stats.calculate_total_duration(ppq).as_secs_f64(),
            Ordering::Relaxed,
        );
    }

    parse_length_outer.load(Ordering::Relaxed)
}

fn pause() {
    let mut stdin = io::stdin();
    let mut stdout = io::stdout();
    write!(stdout, "Press any key to continue...").unwrap();
    stdout.flush().unwrap();
    let _ = stdin.read(&mut [0u8]).unwrap();
}
