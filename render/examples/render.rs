use std::time::Instant;
use xsynth_render::builder::xsynth_renderer;

fn main() {
    let midi = "/home/jim/Black MIDIs/MIDI Files/Infernis/Impossible Piano - EoSD - Septette for the Dead Princess ][ black.mid";
    let sfz = vec!["/home/jim/Black MIDIs/SoundFonts/MBMS Soundfonts/CFaz Keys IV Concert Grand Piano/cfaz1l.sfz"];
    let out = "out.wav";

    let render_time = Instant::now();

    xsynth_renderer(midi, out)
        .with_config(Default::default())
        .add_soundfonts(sfz)
        .with_layer_count(Some(10))
        .run()
        .unwrap();

    println!("Render time: {} seconds", render_time.elapsed().as_secs());
}
