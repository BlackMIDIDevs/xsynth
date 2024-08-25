# xsynth-kdmapi
A cdylib wrapper around XSynth to act as a drop in replacement for OmniMIDI/KDMAPI.

## Usage
1) Download `OmniMIDI.dll` from the [releases](https://github.com/BlackMIDIDevs/xsynth/releases) page
    1) Alternatively you can build the library yourself by cloning the repo and compiling with Cargo.
2) Place it in the same directory as your KDMAPI aware software of choice.

Upon loading the library, the following two files will be generated under `%userprofile%/AppData/Roaming/xsynth-kdmapi` (on Windows):

### `settings.json`
The synthesizer settings. Fields:
- `layers`
    
    - The layer limit for each channel. One layer is one voice per key per channel. If set to `null` the synth will not limit the voices.
    - This setting will be updated live during playback.

- `fade_out_killing`

    - If set to `true`, the voices killed due to the voice limit will fade out. If set to `false`, they will be killed immediately, usually causing clicking but improving performance.

- `render_window_ms`

    - The length of the buffer reader in ms.

- `multithreading`

    - Controls the multithreading used for rendering per-voice audio for all the voices stored in a key for a channel.
    - Can be `"None"` for no multithreading, `"Auto"` for multithreading with an automatically determined thread count, or `{ "Manual": <threads> }` for multithreading with a custom thread count.

- `ignore_range`

    - The synth will ignore notes in this range of velocities.
    - Values: `start` (low velocity), `end` (high velocity).

### `soundfonts.json`
The list of soundfonts that will be used. Any changes in the soundfont list will be updated live during playback.

For information about the supported soundfont formats visit [the official XSynth documentation](https://docs.rs/xsynth-core/latest/xsynth_core/soundfont/struct.SampleSoundfont.html).

Each soundfont item has the following fields:

- `path`

    - The path of the soundfont.

- `enabled`

    - Whether or not the soundfont will be loaded. Can be `true` or `false`.

- `bank`

    - The bank number (0-128) to extract and use from the soundfont.
    - `null` means to use all available banks (bank 0 for SFZ).

- `preset`

    - The preset number (0-127) to extract and use from the soundfont.
    - `null` means to use all available presets (preset 0 for SFZ).

- `vol_envelope_options`

    - Volume envelope configuration in the dB scale. Each option supports the following values: `"Exponential"` and `"Linear"`.
        - `attack_curve`: Attack stage curve type
        - `decay_curve`: Decay stage curve type
        - `release_curve`: Release stage curve type

- `use_effects`

    - If set to `true`, the soundfont will be able to use signal processing effects. Currently this option only affects the cutoff filter. Setting to `false` disables those filters.

- `interpolator`

    - The type of interpolator used in the soundfont.
    - Can be `"Nearest"` for nearest neighbor interpolation (no interpolation) or `"Linear"` for linear interpolation.