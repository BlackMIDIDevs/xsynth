use std::time::Instant;
use xsynth_render::builder::xsynth_renderer;

fn main() {
    let args = std::env::args().collect::<Vec<String>>();
    let (Some(midi), Some(sfz)) =
        (args.get(1).cloned().or_else(|| std::env::var("XSYNTH_EXAMPLE_MIDI").ok()),
         args.get(2).cloned().or_else(|| std::env::var("XSYNTH_EXAMPLE_SFZ").ok())) else {
        println!(
            "Usage: {} [midi] [sfz]",
            std::env::current_exe()
                .unwrap_or("example".into())
                .display()
        );
        return;
    };
    let out = "out.wav";

    let render_time = Instant::now();

    xsynth_renderer(&midi, out)
        .with_config(Default::default())
        .add_soundfonts(vec![sfz.as_str()])
        .with_layer_count(Some(10))
        .run()
        .unwrap();

    println!("Render time: {} seconds", render_time.elapsed().as_secs());
}
