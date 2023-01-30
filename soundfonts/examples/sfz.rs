use xsynth_soundfonts::sfz::grammar::{self};

fn main() {
    let args = std::env::args().collect::<Vec<String>>();
    let path = args.get(1).unwrap();
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
