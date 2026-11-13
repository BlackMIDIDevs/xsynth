<h1 align="center">XSynth</h1>
<p align="center"><b>A fast Rust-based SoundFont synthesizer designed for high voice counts and low latency.</b></p>
<p align="center">
<img alt="GitHub repo size" src="https://img.shields.io/github/repo-size/BlackMIDIDevs/xsynth">
<img alt="GitHub License" src="https://img.shields.io/github/license/BlackMIDIDevs/xsynth">
<img alt="GitHub Release" src="https://img.shields.io/github/v/release/BlackMIDIDevs/xsynth">
</p>

## Modules

- [`core`](https://github.com/BlackMIDIDevs/xsynth/tree/master/core): Handles the core audio rendering functionality.
- [`clib`](https://github.com/BlackMIDIDevs/xsynth/tree/master/clib): C/C++ bindings for XSynth.
- [`soundfonts`](https://github.com/BlackMIDIDevs/xsynth/tree/master/soundfonts): A module to parse soundfonts to be used in XSynth.
- [`realtime`](https://github.com/BlackMIDIDevs/xsynth/tree/master/realtime): The real-time rendering module within XSynth.
- [`render`](https://github.com/BlackMIDIDevs/xsynth/tree/master/render): Offline rendering support, exposed both as a reusable library and a command line utility for rendering MIDIs to audio using XSynth.
- [`kdmapi`](https://github.com/BlackMIDIDevs/xsynth/tree/master/kdmapi): A cdylib wrapper around XSynth to act as a drop in replacement for OmniMIDI/KDMAPI.

## API Surface

Most integrations should use the native Rust crates directly:
- `xsynth-core` for soundfont loading and sample rendering
- `xsynth-realtime` for realtime playback against an audio device
- `xsynth-render` for offline rendering to WAV, either as a library or CLI

The non-Rust entry points exist for compatibility:
- `xsynth-clib` exposes a C ABI intended for legacy integrations
- `xsynth-kdmapi` exposes a KDMAPI-compatible DLL for Windows MIDI stacks

## Architecture

XSynth is designed around a performance-first voice engine. The hot rendering
path is heavily specialized so common voice configurations can stay simple and
compiler-optimized.

That design has an important consequence: features that require a large generic
runtime modulation graph or many optional per-voice stages are intentionally
treated with caution, because they can increase both CPU overhead and binary
size. In practice, XSynth prefers to resolve as much soundfont behavior as
possible up front during load time.

## SFZ Support

XSynth supports a practical subset of SFZ centered around sample-region
playback, amplitude/filter shaping and MIDI controller modulation.

Supported today:
- `lovel`, `hivel`, `lokey`, `hikey`
- `pitch_keycenter`
- `volume`, `pan`, `tune`
- `sample`, `default_path`
- `loop_mode`, `loop_start`, `loop_end`, `offset`
- `cutoff`, `resonance`, `fil_veltrack`, `fil_keycenter`, `fil_keytrack`, `filter_type`
- `ampeg_start`, `ampeg_delay`, `ampeg_attack`, `ampeg_hold`, `ampeg_decay`, `ampeg_sustain`, `ampeg_release`
- `amplitude`
- controller modulation, updated live while voices play: `amplitude_onccN`, `volume_onccN`/`gain_onccN`, `pan_onccN`, `pitch_onccN`/`tune_onccN`, `cutoff_onccN`, `resonance_onccN`, `cutoff_chanaft` (and the SFZ v1 `*_ccN` aliases)
- note-on envelope modulation: `ampeg_start_onccN`, `ampeg_delay_onccN`, `ampeg_attack_onccN`, `ampeg_hold_onccN`, `ampeg_decay_onccN`, `ampeg_sustain_onccN`, `ampeg_release_onccN` (and `ampeg_*ccN`)
- extended controllers `128` (pitch bend) and `129` (channel aftertouch) in `*_onccN` opcodes
- `loccN`/`hiccN` (note-on gating), `on_loccN`/`on_hiccN` (controller-triggered regions), `set_ccN`/`set_hdccN`
- `xfin_loccN`, `xfin_hiccN`, `xfout_loccN`, `xfout_hiccN`, `xf_cccurve`
- `effect1`/`effect2` as reverb/chorus send levels (averaged per channel program)
- nested includes and `#define` substitution in the parser

The SFZ path is intended to cover the common sample-playback workflow well. It
is not a claim of full SFZ opcode coverage.

## SF2 Support

XSynth aims for high-performance practical SF2 playback rather than full
SoundFont 2 spec emulation.

Supported today:
- static sample-region playback, including offsets, loop points, stereo links, root key, tuning, scale tuning, key/velocity ranges, fixed key/velocity, filter cutoff/Q, volume envelope generators, and exclusive class
- a baked subset of note-on modulators, resolved at soundfont load time, covering key/velocity sources with linear/concave/convex/switch curves for practical destinations such as attenuation, filter cutoff, pan, volume envelope timings, and static pitch offsets
- runtime modulators driven by MIDI CCs, channel pressure or the pitch wheel, targeting attenuation, pan, filter cutoff/Q and fine/coarse tune (updated live) or volume envelope timings (applied at note-on); key/velocity amount sources are folded in at load time
- the default CC7/CC10/CC11/pitch wheel/CC91/CC93 modulators are implemented once per channel rather than per voice

Intentionally out of scope for performance and binary-size reasons:
- modulation envelope generators and their destinations
- modulation LFO / vibrato LFO generators and destinations (the channel-level CC1 vibrato is used instead)
- modulators targeting chorus/reverb sends, poly pressure or pitch wheel sensitivity sources, and linked modulators
- per-region SF2 chorus and reverb send generators

## MIDI Controller Support

Controllers are handled per channel, so most of them cost nothing per voice:
- `CC0` bank select, `CC1` modulation wheel (channel vibrato, depth from RPN 5), `CC5`/`CC65`/`CC84` portamento
- `CC6`/`CC38` data entry, `CC96`/`CC97` data increment/decrement, `CC98`-`CC101` NRPN/RPN select (pitch bend range, fine/coarse tune, modulation depth range, RPN null, GS/XG vibrato/filter/envelope NRPNs)
- `CC7` volume, `CC8` balance, `CC10` pan, `CC11` expression
- `CC64` damper, `CC66` sostenuto, `CC67` soft pedal, `CC69` hold 2
- `CC71`-`CC77` resonance, release, attack, cutoff, decay, vibrato rate, vibrato depth
- `CC91`/`CC93` reverb/chorus sends into a shared reverb and chorus, which only run while a channel sends signal to them
- `CC120`, `CC121`, `CC123`-`CC125`, `CC126`/`CC127` channel mode messages
- channel pressure

Every CC value, the pitch wheel and channel pressure are also available to
soundfont modulators. A voice only re-evaluates its modulation when a
controller that the loaded soundfonts actually use changes.

## Realtime And Offline Rendering

- `xsynth-realtime` is the realtime playback layer and is the right choice when
  you need low-latency live MIDI synthesis against an audio device
- `xsynth-render` uses the same synth stack for offline rendering and now
  exposes a reusable library API in addition to the CLI
- `xsynth-core` remains the lowest-level crate for the shared rendering and
  soundfont logic used by both paths

## Demos

#### XSynth playing Immortal Smoke by EpreTroll

https://github.com/user-attachments/assets/d100e3d2-efa0-4367-a774-d5a171ac0bf8

#### XSynth playing DANCE.MID

https://github.com/user-attachments/assets/f509a36c-6019-4d38-9e5e-1bf0eeb9b43d

## License

XSynth and all of its components is licensed under the [GNU Lesser General Public License 3.0](https://www.gnu.org/licenses/lgpl-3.0.en.html#license-text).
