use criterion::{criterion_group, criterion_main, Criterion};
use xsynth_soundfonts::sfz::{grammar, parse_soundfont};

fn criterion_benchmark(c: &mut Criterion) {
    let path =
        "/run/media/d/Midis/Soundfonts/Loud and Proud Remastered/Kaydax Presets/Loud and Proud Remastered.sfz";
    let str = std::fs::read_to_string(path).unwrap();

    c.bench_function("parse sfz tokens", |f| {
        f.iter(|| grammar::ErrorTolerantToken::parse_as_iter(&str).count())
    });

    c.bench_function("parse full sfz", |f| f.iter(|| parse_soundfont(path)));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
