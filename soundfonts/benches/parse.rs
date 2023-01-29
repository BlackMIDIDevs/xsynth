use criterion::{criterion_group, criterion_main, Criterion};
use xsynth_soundfonts::sfz::grammar;

fn criterion_benchmark(c: &mut Criterion) {
    let path =
        "/run/media/d/Midis/Soundfonts/Loud and Proud Remastered/Kaydax Presets/Extensions/L&PR";
    let str = std::fs::read_to_string(path).unwrap();

    c.bench_function("parse sfz", |f| {
        f.iter(|| grammar::Token::parse_as_iter(&str).count())
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
