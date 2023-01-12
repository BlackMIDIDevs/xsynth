<h1 align="center">XSynth</h1>
<p align="center">A fast Rust-based SoundFont synthesizer designed for high voice counts and low latency.</p>

## Modules

### Core
Handles the core audio rendering functionality.
The main components are channels and voices:
- Channels represent a single MIDI channel (normally MIDIs use 16 channels together)
- A voice represents a single SoundFont sound

### Realtime
The real-time rendering module within XSynth. Currently it outputs audio using `cpal`.
It uses an asynchronous event sending system for high performance and simple to use API.

### KDMAPI
A cdylib wrapper around real-time to act as a drop-in replacement for OmniMIDI.

### Rendered
A module for rendering audio to a file.
It takes in a MIDI file path and other XSynth parameters, and outputs a wav file.


## License

XSynth is licensed under the Mozilla Public License Version 2.0.
