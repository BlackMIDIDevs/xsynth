use std::{fs::File, path::Path};

use sofiza::Opcode;
use wav::BitDepth;

fn main() {
    // let sf = SoundFont2::from_data(
    //     SFData::load(&mut File::open("D:/Midis/sf/25 Piano Soundfonts/Giga Piano.sf2").unwrap())
    //         .unwrap(),
    // );
    // dbg!(sf);
    let sfz = sofiza::Instrument::from_file(Path::new(
        "D:/Midis/Steinway-B-211-master/Steinway-B-211-master/Presets/1960 Steinway B-211.sfz",
    ))
    .unwrap();
    let opcode = sfz.regions[100].get("sample").unwrap();

    if let Opcode::sample(sample) = opcode {
        dbg!(&sfz.default_path);
        dbg!(&sample);
        let path = sfz.default_path.join(sample);
        dbg!(&path);
        let mut reader = File::open(path).unwrap();
        let (header, data) = wav::read(&mut reader).unwrap();
        dbg!(header);

        fn build_arrays<T: Copy, F: Fn(T) -> f32>(
            arr: &[T],
            channels: u16,
            cast: F,
        ) -> Vec<Vec<f32>> {
            let mut chans = Vec::new();
            for _ in 0..channels {
                chans.push(Vec::new());
            }

            for i in 0..arr.len() {
                let v = cast(arr[i]);
                chans[i % channels as usize].push(v);
            }

            for chan in chans.iter_mut() {
                chan.shrink_to_fit();
            }

            chans
        }

        fn extract_samples(data: BitDepth, channels: u16) -> Vec<Vec<f32>> {
            match data.as_eight() {
                Some(data) => return build_arrays(data, channels, |v| (v as f32 - 128.0) / 128.0),
                None => {}
            };

            match data.as_sixteen() {
                Some(data) => {
                    return build_arrays(data, channels, |v| (v as f32) / i16::MAX as f32)
                }
                None => {}
            };

            match data.as_thirty_two_float() {
                Some(data) => return build_arrays(data, channels, |v| v),
                None => {}
            };

            match data.as_twenty_four() {
                Some(data) => return build_arrays(data, channels, |v| v as f32 / (1 << 23) as f32),
                None => {}
            }

            panic!()
        }

        let vecs = extract_samples(data, header.channel_count);

        dbg!(vecs);
    }

    // dbg!(opcode);
}
