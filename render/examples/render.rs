use xsynth_render::{config::XSynthRenderConfig, render_to_file};

fn main() {
    let config = XSynthRenderConfig::default();
    let midi = "/home/jim/Black MIDIs/MIDI Files/Danidanijr/4448 By Danidanijr.mid";
    let sfz = "/home/jim/Black MIDIs/SoundFonts/MBMS Soundfonts/CFaz Keys IV Concert Grand Piano/.PianoSamples/cfaz.sfz";
    let out = "out.wav";

    let render_time = render_to_file(config, midi, sfz, out);

    println!("Render Time: {render_time} seconds");
}
