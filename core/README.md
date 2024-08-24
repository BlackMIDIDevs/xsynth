<h1 align="center">XSynth</h1>
<p align="center">A fast Rust-based SoundFont synthesizer designed for high voice counts and low latency.</p>

## Components

### `VoiceChannel`

Represents a single MIDI channel.
Keeps track and manages MIDI events and the active voices of a channel.

Unlike most other MIDI synthesizers that use global voice limits, each XSynth channel limits the spawned voices per-key using "layers". One layer corresponds to one voice per key per channel.

For information about supported events and controllers, please visit the [VoiceChannel documentation](https://docs.rs/xsynth-core/latest/xsynth_core/channel/struct.VoiceChannel.html). 

### `ChannelGroup`

Represents a MIDI synthesizer within XSynth.
Manages multiple `VoiceChannel` objects at once in an easy to use way.

### `SampleSoundfont`

Represents a sample SoundFont to be used within XSynth. Holds the voice and program data, as well as the samples of a SoundFont.

For information about supported formats, please visit the [SampleSoundfont documentation](https://docs.rs/xsynth-core/latest/xsynth_core/soundfont/struct.SampleSoundfont.html).

### `Voice`

A voice represents a single SoundFont sound. They are usually generated within a `VoiceChannel` according to the sent events.

## Documentation

You can find all the necessary documentation about the XSynth API here: [https://docs.rs/xsynth-core](https://docs.rs/xsynth-core).