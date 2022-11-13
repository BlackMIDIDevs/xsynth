use std::time::Instant;
use xsynth_render::builder::{xsynth_renderer, XSynthRenderStats};

fn main() {
    let midi = "/home/jim/Black MIDIs/MIDI Files/Infernis/Impossible Piano - EoSD - Septette for the Dead Princess ][ black.mid";
    let sfz = vec!["/home/jim/Black MIDIs/SoundFonts/MBMS Soundfonts/CFaz Keys IV Concert Grand Piano/.PianoSamples/cfaz.sfz"];
    let out = "out.wav";

    let callback = |stats: XSynthRenderStats| {
        print!("\rMIDI position: {}", stats.progress);
    };

    let render_time = Instant::now();

    xsynth_renderer(midi, out)
        .with_config(Default::default())
        .add_soundfonts(sfz)
        .with_progress_callback(callback)
        .run();

    println!(
        "\nDone! Render time: {} seconds",
        render_time.elapsed().as_secs()
    );
}
