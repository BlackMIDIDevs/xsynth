# XSynth

A Rust-based soundfont synthesizer designed for extremely high voice counts and low latency. In some tests, it achieved over 8k voices at 1ms sampling.

XSynth currently has the following components:

### Core
The core module, which describes core audio rendering functionality. The main components are channels and voices, where channels represent a single MIDI channel (usually a synth would use 16 MIDI channels together), and a voice represents a single soundfont sound.

### Realtime
The realtime rendering module within XSynth. Currently it outputs audio using cpal. It uses an asynchronous event sending system for high performance and simple to use API.

### KDMAPI
A cdylib wrapper around realtime to act as a drop-in replacement for OmniMIDI.

### Rendered
A module for rendering audio to a file. It takes in a midi file path and other xsynth parameters, and outputs a wav file.