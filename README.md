<h1 align="center">XSynth</h1>
<p align="center"><b>A fast Rust-based SoundFont synthesizer designed for high voice counts and low latency.</b></p>
<p align="center">
<img alt="GitHub repo size" src="https://img.shields.io/github/repo-size/BlackMIDIDevs/xsynth">
<img alt="GitHub License" src="https://img.shields.io/github/license/BlackMIDIDevs/xsynth">
<img alt="GitHub Release" src="https://img.shields.io/github/v/release/BlackMIDIDevs/xsynth">
</p>

## Modules

- [`core`](/core): Handles the core audio rendering functionality.
- [`clib`](/clib): C/C++ bindings for XSynth.
- [`soundfonts`](/soundfonts): A module to parse soundfonts to be used in XSynth.
- [`realtime`](/realtime): The real-time rendering module within XSynth.
- [`render`](/render): A command line utility for rendering MIDIs to audio using XSynth.
- [`kdmapi`](/kdmapi): A cdylib wrapper around XSynth to act as a drop in replacement for OmniMIDI/KDMAPI.
- [`interface`](/interface): A MIDI interface for XSynth.

## Demos

#### XSynth playing Immortal Smoke by EpreTroll

https://github.com/user-attachments/assets/d100e3d2-efa0-4367-a774-d5a171ac0bf8

#### XSynth playing DANCE.MID

https://github.com/user-attachments/assets/f509a36c-6019-4d38-9e5e-1bf0eeb9b43d

## License

XSynth and all of its components is licensed under the [GNU Lesser General Public License 3.0](https://www.gnu.org/licenses/lgpl-3.0.en.html#license-text).
