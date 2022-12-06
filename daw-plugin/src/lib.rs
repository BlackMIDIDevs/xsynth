use nih_plug::prelude::*;
use std::sync::Arc;

// This is a shortened version of the gain example with most comments removed, check out
// https://github.com/robbert-vdh/nih-plug/blob/master/plugins/examples/gain/src/lib.rs to get
// started

struct XSynthDawPlugin {
    params: Arc<XSynthDawPluginParams>,
    sample_rate: f32,

    /// The current phase of the sine wave, always kept between in `[0, 1]`.
    phase: f32,

    /// The MIDI note ID of the active note, if triggered by MIDI.
    midi_note_id: u8,
    /// The frequency if the active note, if triggered by MIDI.
    midi_note_freq: f32,
    /// A simple attack and release envelope to avoid clicks. Controlled through velocity and
    /// aftertouch.
    ///
    /// Smoothing is built into the parameters, but you can also use them manually if you need to
    /// smooth soemthing that isn't a parameter.
    midi_note_gain: Smoother<f32>,
}

#[derive(Params)]
struct XSynthDawPluginParams {
    #[id = "gain"]
    pub gain: FloatParam,

    #[id = "freq"]
    pub frequency: FloatParam,

    #[id = "usemid"]
    pub use_midi: BoolParam,
}

impl Default for XSynthDawPluginParams {
    fn default() -> Self {
        Self {
            gain: FloatParam::new(
                "Gain",
                -10.0,
                FloatRange::Linear {
                    min: -30.0,
                    max: 0.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(3.0))
            .with_step_size(0.01)
            .with_unit(" dB"),
            frequency: FloatParam::new(
                "Frequency",
                420.0,
                FloatRange::Skewed {
                    min: 1.0,
                    max: 20_000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_smoother(SmoothingStyle::Linear(10.0))
            // We purposely don't specify a step size here, but the parameter should still be
            // displayed as if it were rounded. This formatter also includes the unit.
            .with_value_to_string(formatters::v2s_f32_hz_then_khz(0))
            .with_string_to_value(formatters::s2v_f32_hz_then_khz()),
            use_midi: BoolParam::new("Use MIDI", true),
        }
    }
}

impl Default for XSynthDawPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(XSynthDawPluginParams::default()),
            sample_rate: 1.0,

            phase: 0.0,

            midi_note_id: 0,
            midi_note_freq: 1.0,
            midi_note_gain: Smoother::new(SmoothingStyle::Linear(5.0)),
        }
    }
}

impl XSynthDawPlugin {
    fn calculate_sine(&mut self, frequency: f32) -> f32 {
        let phase_delta = frequency / self.sample_rate;
        let sine = (self.phase * std::f32::consts::TAU).sin();

        self.phase += phase_delta;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        sine
    }
}

impl Plugin for XSynthDawPlugin {
    const NAME: &'static str = "XSynth";
    const VENDOR: &'static str = "Arduano";
    const URL: &'static str = env!("CARGO_PKG_HOMEPAGE");
    const EMAIL: &'static str = "your@email.com";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const DEFAULT_INPUT_CHANNELS: u32 = 0;
    const DEFAULT_OUTPUT_CHANNELS: u32 = 2;

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn accepts_bus_config(&self, config: &BusConfig) -> bool {
        // This can output to any number of channels, but it doesn't take any audio inputs
        config.num_input_channels == 0 && config.num_output_channels > 0
    }

    fn initialize(
        &mut self,
        _bus_config: &BusConfig,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;

        true
    }

    fn reset(&mut self) {
        self.phase = 0.0;
        self.midi_note_id = 0;
        self.midi_note_freq = 1.0;
        self.midi_note_gain.reset(0.0);
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let mut next_event = context.next_event();

        for (sample_id, channel_samples) in buffer.iter_samples().enumerate() {
            // Smoothing is optionally built into the parameters themselves
            let gain = self.params.gain.smoothed.next();

            // This plugin can be either triggered by MIDI or controleld by a parameter
            let sine = if self.params.use_midi.value() {
                // Act on the next MIDI event
                while let Some(event) = next_event {
                    if event.timing() > sample_id as u32 {
                        break;
                    }

                    match event {
                        NoteEvent::NoteOn { note, velocity, .. } => {
                            self.midi_note_id = note;
                            self.midi_note_freq = util::midi_note_to_freq(note);
                            self.midi_note_gain.set_target(self.sample_rate, velocity);
                        }
                        NoteEvent::NoteOff { note, .. } if note == self.midi_note_id => {
                            self.midi_note_gain.set_target(self.sample_rate, 0.0);
                        }
                        NoteEvent::PolyPressure { note, pressure, .. }
                            if note == self.midi_note_id =>
                        {
                            self.midi_note_gain.set_target(self.sample_rate, pressure);
                        }
                        _ => (),
                    }

                    next_event = context.next_event();
                }

                // This gain envelope prevents clicks with new notes and with released notes
                self.calculate_sine(self.midi_note_freq) * self.midi_note_gain.next()
            } else {
                let frequency = self.params.frequency.smoothed.next();
                self.calculate_sine(frequency)
            };

            for sample in channel_samples {
                *sample = sine * util::db_to_gain(gain);
            }
        }

        ProcessStatus::KeepAlive
    }
}

impl ClapPlugin for XSynthDawPlugin {
    const CLAP_ID: &'static str = "com.xsynth.dawplugin";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("A short description of your plugin");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;

    // Don't forget to change these features
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
        ClapFeature::Sampler,
    ];
}

impl Vst3Plugin for XSynthDawPlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"jMFKXFgY9JsYyDq4";

    // And don't forget to change these categories, see the docstring on `VST3_CATEGORIES` for more
    // information
    const VST3_CATEGORIES: &'static str = "Instrument|Synth|Sampler";
}

nih_export_clap!(XSynthDawPlugin);
nih_export_vst3!(XSynthDawPlugin);
