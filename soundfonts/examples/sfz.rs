use xsynth_soundfonts::sfz::grammar::{self};

fn main() {
    let path = "/home/jim/Projects/SoundFonts/AIG/OUT/Amethyst Imperial Grand - Imperial Hall/Amethyst Imperial Grand - Imperial Hall.sfz";
    let str = std::fs::read_to_string(path).unwrap();
    dbg!("Parsing");

    let result = grammar::Root::parse_full(&str);

    match result {
        Ok(val) => {
            println!("{val:#?}");
        }
        Err(err) => println!("Error: {err}"),
    }
}
