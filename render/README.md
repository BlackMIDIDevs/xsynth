# xsynth-render

A command line utility for rendering MIDIs to audio using XSynth.
It receives a MIDI file path and other parameters as arguments, and generates an audio file in WAV format.

Use by running `cargo run -r -- <arguments>` if you are compiling from source
or `xsynth-render <arguments>` if you are using a pre-built binary.

## Arguments

You can view all the available options by running `xsynth-render --help`:

```
Usage: xsynth-render [OPTIONS] <midi> <soundfonts>...

Arguments:
  <midi>           The path of the MIDI file to be converted.
  <soundfonts>...  Paths of the soundfonts to be used.
                   Will be loaded in the order they are typed.

Options:
  -o, --output <output>
          The path of the output audio file.
          Default: "out.wav"
  -s, --sample-rate <sample rate>
          The sample rate of the output audio in Hz.
          Default: 48000 (48kHz)
  -c, --audio-channels <audio channels>
          The audio channel count of the output audio.
          Supported: "mono" and "stereo"
          Default: stereo
  -l, --layers <layer limit>
          The layer limit for each channel. Use "0" for unlimited layers.
          One layer is one voice per key per channel.
          Default: 32
      --channel-threading <channel threading>
          Per-channel multithreading options.
          Use "none" for no multithreading, "auto" for multithreading with
          an automatically determined thread count or any number to specify the
          amount of threads that should be used.
          Default: "auto"
      --key-threading <key threading>
          Per-key multithreading options.
          Use "none" for no multithreading, "auto" for multithreading with
          an automatically determined thread count or any number to specify the
          amount of threads that should be used.
          Default: "auto"
  -L, --apply-limiter
          Apply an audio limiter to the output audio to prevent clipping.
      --disable-fade-out
          Disables fade out when killing a voice. This may cause popping.
      --linear-envelope
          Use a linear decay and release phase in the volume envelope, in amplitude units.
  -I, --interpolation <interpolation>
          The interpolation algorithm to use. Available options are
          "none" (no interpolation) and "linear" (linear interpolation).
          Default: "linear"
  -h, --help
          Print help
  -V, --version
          Print version
```