# xsynth-realtime

The real-time rendering module within XSynth. Currently it outputs audio using `cpal`.

It uses an asynchronous event sending system for high performance and simple to use API.

## Documentation

You can find all the necessary documentation about the XSynth realtime API here: [https://docs.rs/xsynth-realtime](https://docs.rs/xsynth-realtime).

## Example

This is a very simple example about initializing an instance of the realtime synthesizer. For other more detailed use cases, visit the [examples folder](https://github.com/BlackMIDIDevs/xsynth/tree/master/realtime/examples).

```rust
use xsynth_realtime::{RealtimeSynth, SynthEvent, ChannelEvent, ChannelAudioEvent};

fn main() {
    // Will use the default configuration and
    // default audio output device
    let mut synth = RealtimeSynth::open_with_all_defaults();
    
    // Will send a note on event in channel 0
    synth.send_event(SynthEvent::Channel(
        0,
        ChannelEvent::Audio(ChannelAudioEvent::NoteOn {
            key: 60,
            vel: 127,
        }),
    ));

    // Will print the active voice count
    println!("Voice Count: {}", synth.get_stats().voice_count());

    // The synth is automatically dropped here
}
```