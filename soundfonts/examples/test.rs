use xsynth_soundfonts::sfz::grammar::{self};

fn main() {
    let path = "/run/media/d/Midis/Soundfonts/Steinway-B-211-master/Steinway-B-211-master/Presets/1960 Steinway B-211.sfz";
    let str = std::fs::read_to_string(path).unwrap();
    dbg!("Parsing");

    // let result = grammar::Root::parse_full(&str);
    // dbg!(result);

    let count = grammar::Token::parse_as_iter(&str).count();
    dbg!(count);
}
