# XSynth

Rust SoundFont synthesizer workspace (SF2 + SFZ). LGPL-3.0. See `README.md` for
module list and the SF2/SFZ support boundaries — those lists are authoritative
for what is intentionally out of scope.

## Commands

- Toolchain pinned to `nightly` by `rust-toolchain`. Use nightly for everything.
- CI gate (`.github/workflows/check.yml`), run in this order:
  1. `cargo fmt --all -- --check`
  2. `cargo clippy --workspace --all-targets --all-features -- --deny "warnings"`
  3. `cargo check --workspace --all-targets --all-features`
  Clippy warnings fail the build, so lint noise in a touched crate is blocking.
- Tests: `cargo test --workspace`. All tests are inline `#[cfg(test)]` modules —
  there is no `tests/` directory. Scope with `cargo test -p xsynth-soundfonts sfz`
  or pass a single test name.
- Linux builds need ALSA headers for `cpal` (`libasound2-dev`). CI and `Cross.toml`
  also install `portaudio19-dev libpulse-dev libdbus-1-dev`.
- Render CLI: `cargo run -p xsynth-render --release -- <midi> <soundfonts...>`
  (options in `render/README.md`).
- Realtime demo and benches need sound/audio files that are **not** committed:
  - `XSYNTH_EXAMPLE_MIDI=/path/x.mid XSYNTH_EXAMPLE_SF=/path/x.sf2 cargo run -p xsynth-realtime --example midi`
  - `XSYNTH_EXAMPLE_SFZ=/path/x.sfz cargo bench -p xsynth-core`
  With the env var unset they print usage and exit 0 — a silent skip, not a pass.

## Layout

- Members: `soundfonts` → `core` → `realtime` / `render`; `clib` and `kdmapi` are
  thin ABI wrappers on top. Only `core`, `soundfonts`, `realtime` go to crates.io
  (`clib`, `render`, `kdmapi` set `publish = false`).
- Single version source: `[workspace.package] version`. Internal deps are pinned
  exact (`xsynth-core = "=0.4.0"`) in root `[workspace.dependencies]`, so a bump
  edits both places plus `Cargo.lock`.
- `AudioPipe` (`core/src/audio_pipe.rs`) is the seam everything else hangs off:
  `read_samples` also advances time for events already sent. `realtime` feeds it
  from a cpal callback with an async event-sender path; `render::OfflineWavRenderer`
  drains it to WAV.
- Voice limiting is per channel and per key via "layers" (one layer = one voice per
  key per channel), not a global voice pool. `VoiceChannel` = one MIDI channel,
  `ChannelGroup` = one synth holding many channels.
- Hot DSP lives inside `simdeez` macros (`simd_runtime_generate!`, `simd_invoke!`)
  that expand once per ISA. Code there must compile for SSE/AVX/NEON and stay
  allocation-free; expect macro-scoped, confusing type errors when it does not.
- Design bias (README "Architecture"): resolve soundfont behavior at load time,
  not as a generic runtime modulation graph. `serde` is an opt-in feature on
  `core`/`realtime`, enabled by `kdmapi`; test with `--all-features`.

## Gotchas

- Building `xsynth-clib` runs cbindgen (`clib/build.rs`) and writes `clib/xsynth.h`
  into the source tree. It is gitignored but is a release artifact. Doc comments on
  the public clib API become header documentation — change them there, rebuild to
  regenerate the header. `XSYNTH_VERSION` in the header comes from `pkg-version`.
- `kdmapi` builds as `OmniMIDI.dll`; ordinal linking (`kdmapi/Ordinals.def`)
  only applies on Windows targets, per its `build.rs`. It reads
  `settings.json` / `soundfonts.json` from `%APPDATA%/xsynth-kdmapi` and hot-reloads
  them while playing.

## Release

- Bump `[workspace.package] version` plus the exact `=x.y.z` path deps in root
  `Cargo.toml`, refresh `Cargo.lock`; commit subject style is `Prepare x.y.z release`.
- Push to `master` runs `publish.yml` (crates.io). Pushing a tag runs `deploy.yml`:
  cross-built clib libraries, `xsynth-render` binaries and `OmniMIDI.dll` into a
  draft GitHub release. Cross builds use `cross`; its system deps are in `Cross.toml`.
