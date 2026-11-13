//! End-to-end tests for controller-driven soundfont behaviour.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent, ControlEvent, VoiceChannel},
    soundfont::{SampleSoundfont, SoundfontBase, SoundfontInitOptions},
    AudioPipe, AudioStreamParams, ChannelCount,
};

const SAMPLE_RATE: u32 = 48_000;

fn temp_dir(label: &str) -> PathBuf {
    let unique = format!(
        "xsynth-cc-test-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let path = std::env::temp_dir().join(unique);
    fs::create_dir_all(&path).unwrap();
    path
}

/// Writes a 16 bit mono WAV with a constant value, two seconds long.
fn write_wav(path: &Path) {
    let frames = SAMPLE_RATE as usize * 2;
    let data_len = (frames * 2) as u32;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for _ in 0..frames {
        bytes.extend_from_slice(&16384i16.to_le_bytes());
    }
    fs::write(path, bytes).unwrap();
}

fn channel_with_sfz(label: &str, sfz: &str) -> (VoiceChannel, PathBuf) {
    let dir = temp_dir(label);
    write_wav(&dir.join("tone.wav"));
    let sfz_path = dir.join("test.sfz");
    fs::write(&sfz_path, sfz).unwrap();

    let params = AudioStreamParams::new(SAMPLE_RATE, ChannelCount::Stereo);
    let options = SoundfontInitOptions {
        use_effects: true,
        ..Default::default()
    };
    let sf: Arc<dyn SoundfontBase> =
        Arc::new(SampleSoundfont::new_sfz(&sfz_path, params, options).unwrap());

    let mut channel = VoiceChannel::new(Default::default(), params, None);
    channel.process_event(ChannelEvent::Config(ChannelConfigEvent::SetSoundfonts(
        vec![sf],
    )));
    channel.process_event(ChannelEvent::Config(ChannelConfigEvent::SetLayerCount(
        None,
    )));
    (channel, dir)
}

fn cc(channel: &mut VoiceChannel, controller: u8, value: u8) {
    channel.process_event(ChannelEvent::Audio(ChannelAudioEvent::Control(
        ControlEvent::Raw(controller, value),
    )));
}

fn note_on(channel: &mut VoiceChannel, key: u8) {
    channel.process_event(ChannelEvent::Audio(ChannelAudioEvent::NoteOn {
        key,
        vel: 127,
    }));
}

/// Renders 20ms and returns the peak level of the last 5ms.
fn render_level(channel: &mut VoiceChannel) -> f32 {
    let mut out = vec![0.0; (SAMPLE_RATE as usize / 50) * 2];
    channel.read_samples(&mut out);
    out[out.len() * 3 / 4..]
        .iter()
        .fold(0.0f32, |acc, s| acc.max(s.abs()))
}

const ENVELOPE: &str = "ampeg_attack=0 ampeg_release=0.001 loop_mode=no_loop";

#[test]
fn amplitude_oncc_follows_controller_while_playing() {
    let (mut channel, dir) = channel_with_sfz(
        "amplitude",
        &format!("<region> sample=tone.wav {ENVELOPE} amplitude=0 amplitude_oncc1=100\n"),
    );

    note_on(&mut channel, 60);
    let silent = render_level(&mut channel);
    cc(&mut channel, 1, 127);
    let loud = render_level(&mut channel);
    cc(&mut channel, 1, 64);
    let half = render_level(&mut channel);

    assert!(silent < 1e-4, "silent = {silent}");
    assert!(loud > 0.1, "loud = {loud}");
    assert!((half / loud - 64.0 / 127.0).abs() < 0.02, "{half} / {loud}");

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn volume_oncc_and_crossfade_change_gain() {
    let (mut channel, dir) = channel_with_sfz(
        "volume",
        &format!(
            "<region> sample=tone.wav {ENVELOPE} volume_oncc2=-20 xfout_locc3=0 xfout_hicc3=127 xf_cccurve=gain\n"
        ),
    );

    note_on(&mut channel, 60);
    let base = render_level(&mut channel);
    cc(&mut channel, 2, 127);
    let quiet = render_level(&mut channel);
    assert!((quiet / base - 0.1).abs() < 0.01, "{quiet} / {base}");

    cc(&mut channel, 2, 0);
    cc(&mut channel, 3, 127);
    let faded = render_level(&mut channel);
    assert!(faded < 1e-4, "faded = {faded}");

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn locc_hicc_gate_note_on() {
    let (mut channel, dir) = channel_with_sfz(
        "locc",
        &format!("<region> sample=tone.wav {ENVELOPE} locc20=64 hicc20=127\n"),
    );

    note_on(&mut channel, 60);
    assert!(render_level(&mut channel) < 1e-4);
    assert_eq!(channel.get_channel_stats().voice_count(), 0);

    cc(&mut channel, 20, 100);
    note_on(&mut channel, 61);
    assert!(render_level(&mut channel) > 0.1);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn on_locc_triggers_region_without_notes() {
    let (mut channel, dir) = channel_with_sfz(
        "on-locc",
        &format!(
            "<region> sample=tone.wav {ENVELOPE} key=-1 pitch_keycenter=60 on_locc64=64 on_hicc64=127\n"
        ),
    );

    note_on(&mut channel, 60);
    assert!(render_level(&mut channel) < 1e-4);

    cc(&mut channel, 64, 127);
    assert!(render_level(&mut channel) > 0.1);
    assert_eq!(channel.get_channel_stats().voice_count(), 1);

    // Staying inside the range does not retrigger
    cc(&mut channel, 64, 100);
    render_level(&mut channel);
    assert_eq!(channel.get_channel_stats().voice_count(), 1);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn set_cc_provides_initial_controller_values() {
    let (mut channel, dir) = channel_with_sfz(
        "set-cc",
        &format!(
            "<control> set_cc1=127\n<region> sample=tone.wav {ENVELOPE} amplitude=0 amplitude_oncc1=100\n"
        ),
    );

    note_on(&mut channel, 60);
    assert!(render_level(&mut channel) > 0.1);

    // Reset All Controllers goes back to the soundfont defaults
    cc(&mut channel, 1, 0);
    cc(&mut channel, 121, 0);
    note_on(&mut channel, 62);
    assert!(render_level(&mut channel) > 0.1);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn pan_oncc_moves_voice() {
    let (mut channel, dir) = channel_with_sfz(
        "pan",
        &format!("<region> sample=tone.wav {ENVELOPE} pan=-100 pan_oncc30=200\n"),
    );
    note_on(&mut channel, 60);
    let mut out = vec![0.0; (SAMPLE_RATE as usize / 50) * 2];
    channel.read_samples(&mut out);
    let (l, r) = (out[out.len() - 2], out[out.len() - 1]);
    assert!(l > 0.1 && r.abs() < 1e-3, "left only: {l} {r}");

    cc(&mut channel, 30, 127);
    channel.read_samples(&mut out);
    let (l, r) = (out[out.len() - 2], out[out.len() - 1]);
    assert!(r > 0.1 && l.abs() < 1e-3, "right only: {l} {r}");

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn sostenuto_holds_only_captured_notes() {
    let (mut channel, dir) = channel_with_sfz(
        "sostenuto",
        &format!("<region> sample=tone.wav {ENVELOPE}\n"),
    );

    note_on(&mut channel, 60);
    render_level(&mut channel);
    cc(&mut channel, 66, 127);
    note_on(&mut channel, 62);
    render_level(&mut channel);

    for key in [60, 62] {
        channel.process_event(ChannelEvent::Audio(ChannelAudioEvent::NoteOff { key }));
    }
    render_level(&mut channel);
    render_level(&mut channel);
    // Only the note held when the pedal went down remains
    assert_eq!(channel.get_channel_stats().voice_count(), 1);

    cc(&mut channel, 66, 0);
    render_level(&mut channel);
    render_level(&mut channel);
    assert_eq!(channel.get_channel_stats().voice_count(), 0);

    fs::remove_dir_all(dir).unwrap();
}
