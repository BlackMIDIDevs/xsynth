<h1 align="center">XSynth</h1>
<p align="center">A fast Rust-based SoundFont synthesizer designed for high voice counts and low latency.</p>

## Modules

### Core
Handles the core audio rendering functionality.
The main components are:
- `VoiceChannel`: Channels represent a single MIDI channel
- `ChannelGroup`: A channel group represents a manager of channels (MIDI synthesizer)
- `SampleSoundfont`: Holds the data and samples from an SFZ or SF2 soundfont
- `Voice`: A voice represents a single SoundFont sound

### Realtime
The real-time rendering module within XSynth. Currently it outputs audio using `cpal`.
It uses an asynchronous event sending system for high performance and simple to use API.

### Rendered
A module for rendering audio to a file.
It takes in a MIDI file path and other XSynth parameters, and outputs an audio file.

### Soundfonts
A module to parse different types of soundfonts to be used in XSynth.
Currently supports SFZ and SF2 soundfonts. For detailed information about
what is supported, please visit the `SampleSoundfont` documentation in `core`.

## License

XSynth is licensed under the MIT license.
