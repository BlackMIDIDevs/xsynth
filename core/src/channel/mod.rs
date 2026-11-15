use std::sync::{atomic::AtomicU64, Arc};

use crate::{
    effects::MultiChannelBiQuad,
    helpers::{prepapre_cache_vec, sum_simd},
    voice::VoiceControlData,
    AudioStreamParams, ChannelCount,
};

use xsynth_soundfonts::FilterType;

use self::{control::ControlEventData, key::KeyData, params::VoiceChannelParams};

use super::AudioPipe;

use rayon::prelude::*;

mod channel_sf;
mod control;
mod key;
mod params;
mod voice_buffer;
mod voice_spawner;

mod event;
pub use event::*;

#[cfg(test)]
mod cc_tests;

pub(crate) use control::ValueLerp;

/// Minimum amount of work (voices and events times samples) in one block for
/// the keys of a channel to be rendered on the thread pool.
const PARALLEL_MIN_WORK: usize = 512 * 64 * 2;
pub use params::VoiceChannelStatsReader;

struct Key {
    data: KeyData,
    audio_cache: Vec<f32>,
    /// Whether `audio_cache` holds this key's output for the current block
    rendered: bool,
    event_cache: Vec<KeyNoteEvent>,
}

impl Key {
    pub fn new(
        key: u8,
        shared_voice_counter: Arc<AtomicU64>,
        options: ChannelInitOptions,
        channels: usize,
    ) -> Self {
        Key {
            data: KeyData::new(key, shared_voice_counter, options, channels),
            audio_cache: Vec::new(),
            rendered: false,
            event_cache: Vec::new(),
        }
    }
}

/// Options for initializing a new VoiceChannel.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Deserialize, serde::Serialize),
    serde(default)
)]
pub struct ChannelInitOptions {
    /// If set to true, the voices killed due to the voice limit will fade out.
    /// If set to false, they will be killed immediately, usually causing clicking
    /// but improving performance.
    ///
    /// Default: `false`
    pub fade_out_killing: bool,
}

#[allow(clippy::derivable_impls)]
impl Default for ChannelInitOptions {
    fn default() -> Self {
        Self {
            fade_out_killing: false,
        }
    }
}

/// Represents a single MIDI channel within XSynth.
///
/// Keeps track and manages MIDI events and the active voices of a channel.
///
/// MIDI CC Support Chart:
/// - `CC0`: Bank Select
/// - `CC1`: Modulation wheel (vibrato, depth set by RPN 5)
/// - `CC5`: Portamento time
/// - `CC6`, `CC38`: Data entry MSB/LSB
/// - `CC7`: Volume
/// - `CC8`: Balance
/// - `CC10`: Pan
/// - `CC11`: Expression
/// - `CC64`: Damper pedal
/// - `CC65`: Portamento on/off
/// - `CC66`: Sostenuto
/// - `CC67`: Soft pedal
/// - `CC69`: Hold 2
/// - `CC71`: Cutoff resonance
/// - `CC72`: Release time multiplier
/// - `CC73`: Attack time multiplier
/// - `CC74`: Cutoff frequency
/// - `CC75`: Decay time multiplier
/// - `CC76`: Vibrato rate
/// - `CC77`: Vibrato depth
/// - `CC84`: Portamento control
/// - `CC91`: Reverb send (rendered by `ChannelGroup`)
/// - `CC93`: Chorus send (rendered by `ChannelGroup`)
/// - `CC96`, `CC97`: Data increment/decrement
/// - `CC98`, `CC99`: NRPN select (GS/XG vibrato, filter and envelope NRPNs)
/// - `CC100`, `CC101`: RPN select (pitch bend range, fine/coarse tune,
///   modulation depth range, null)
/// - `CC120`: All sounds off
/// - `CC121`: Reset all controllers
/// - `CC123`, `CC124`, `CC125`: All notes off
/// - `CC126`, `CC127`: Mono / poly mode
///
/// Every controller value, the pitch wheel and channel pressure are also
/// tracked so soundfonts can use them: SFZ `*_onccN`, `loccN`/`hiccN`,
/// `on_loccN`/`on_hiccN`, `xfin_*`/`xfout_*` and SF2 CC modulators.
pub struct VoiceChannel {
    key_voices: Vec<Key>,

    params: VoiceChannelParams,
    threadpool: Option<Arc<rayon::ThreadPool>>,

    stream_params: AudioStreamParams,

    /// The helper struct for keeping track of MIDI control event data
    control_event_data: ControlEventData,

    /// Processed control data, ready to feed to voices
    voice_control_data: VoiceControlData,

    /// Effects
    cutoff: MultiChannelBiQuad,
}

impl VoiceChannel {
    /// Initializes a new voice channel.
    ///
    /// - `options`: Channel configuration
    /// - `stream_params`: Parameters of the output audio
    /// - `threadpool`: The thread-pool that will be used to render the individual
    ///   keys' voices concurrently. If None is used, the voices will be
    ///   rendered on the same thread.
    pub fn new(
        options: ChannelInitOptions,
        stream_params: AudioStreamParams,
        threadpool: Option<Arc<rayon::ThreadPool>>,
    ) -> VoiceChannel {
        fn fill_key_array<T, F: Fn(u8) -> T>(func: F) -> Vec<T> {
            let mut vec = Vec::with_capacity(128);
            for i in 0..128 {
                vec.push(func(i));
            }
            vec
        }

        let params = VoiceChannelParams::new(stream_params);
        let shared_voice_counter = params.stats.voice_counter.clone();

        VoiceChannel {
            params,
            key_voices: fill_key_array(|i| {
                Key::new(
                    i,
                    shared_voice_counter.clone(),
                    options,
                    stream_params.channels.count() as usize,
                )
            }),

            threadpool,

            stream_params,

            control_event_data: ControlEventData::new_defaults(stream_params.sample_rate),
            voice_control_data: VoiceControlData::new_defaults(),

            cutoff: MultiChannelBiQuad::new(
                stream_params.channels.count() as usize,
                FilterType::LowPass,
                stream_params.sample_rate as f32 / 2.0,
                stream_params.sample_rate as f32,
                None,
            ),
        }
    }

    fn apply_channel_effects(&mut self, out: &mut [f32]) {
        let control = &mut self.control_event_data;

        match self.stream_params.channels {
            ChannelCount::Mono => {
                // Volume
                for sample in out.iter_mut() {
                    let vol = control.volume.get_next() * control.expression.get_next();
                    let vol = vol.powi(2);
                    *sample *= vol;
                }
            }
            ChannelCount::Stereo => {
                // Volume
                for sample in out.chunks_mut(2) {
                    let vol = control.volume.get_next() * control.expression.get_next();
                    let vol = vol.powi(2);
                    sample[0] *= vol;
                    sample[1] *= vol;
                }

                // Pan
                for sample in out.chunks_mut(2) {
                    let pan = control.pan.get_next();
                    sample[0] *= ((pan * std::f32::consts::PI / 2.0).cos()).min(1.0);
                    sample[1] *= ((pan * std::f32::consts::PI / 2.0).sin()).min(1.0);
                }
            }
        }

        // Cutoff
        if let Some(cutoff) = control.cutoff {
            self.cutoff
                .set_filter_type(FilterType::LowPass, cutoff, control.resonance);
            self.cutoff.process(out);
        }
    }

    /// Returns the (reverb, chorus) send levels of the channel, between 0 and 1.
    ///
    /// The sends follow CC91 and CC93, scaled by the send levels requested by the
    /// active soundfont regions (SFZ `effect1`/`effect2`). They are consumed by
    /// `ChannelGroup`, which owns the shared reverb and chorus effects.
    pub fn effect_sends(&self) -> (f32, f32) {
        let (reverb_scale, chorus_scale) = self.params.channel_sf.effect_scale();
        (
            self.control_event_data.reverb * reverb_scale,
            self.control_event_data.chorus * chorus_scale,
        )
    }

    fn push_key_events_and_render(&mut self, out: &mut [f32]) {
        self.params.load_program();

        if self.vibrato_active() {
            // The vibrato LFO modulates the channel pitch, so render in short blocks
            // and update the pitch between them.
            let channels = self.stream_params.channels.count() as usize;
            let block = control::VIBRATO_BLOCK_FRAMES * channels;
            for chunk in out.chunks_mut(block) {
                self.advance_vibrato(chunk.len() / channels);
                self.render_keys(chunk);
            }
        } else {
            self.render_keys(out);
        }

        self.apply_channel_effects(out);
    }

    fn render_keys(&mut self, out: &mut [f32]) {
        out.fill(0.0);

        // Handing work to the thread pool costs tens of microseconds, more than
        // rendering a few voices for a short buffer, so small blocks are
        // rendered on the calling thread.
        let mut work = 0;
        for key in self.key_voices.iter_mut() {
            key.rendered = false;
            work += key.data.voice_count() + key.event_cache.len();
        }
        let work = work * out.len();

        match self.threadpool.as_ref() {
            Some(pool) if work >= PARALLEL_MIN_WORK => {
                let len = out.len();
                let key_voices = &mut self.key_voices;
                let params = &self.params;
                let control_data = &self.voice_control_data;
                pool.install(|| {
                    key_voices
                        .par_iter_mut()
                        .filter(|key| key.data.has_voices() || !key.event_cache.is_empty())
                        .for_each(move |key| {
                            for e in key.event_cache.drain(..) {
                                key.data.send_event(
                                    e,
                                    control_data,
                                    &params.channel_sf,
                                    params.layers,
                                );
                            }

                            // Silent keys skip clearing and summing their buffer.
                            key.rendered = key.data.has_voices();
                            if key.rendered {
                                prepapre_cache_vec(&mut key.audio_cache, len, 0.0);
                                key.data.render_to(&mut key.audio_cache);
                            } else {
                                key.data.render_to(&mut []);
                            }
                        });
                });

                for key in self.key_voices.iter().filter(|key| key.rendered) {
                    sum_simd(&key.audio_cache, out);
                }
            }
            _ => {
                for key in self.key_voices.iter_mut() {
                    for e in key.event_cache.drain(..) {
                        key.data.send_event(
                            e,
                            &self.voice_control_data,
                            &self.params.channel_sf,
                            self.params.layers,
                        );
                    }

                    key.data.render_to(out);
                }
            }
        }
    }

    fn propagate_voice_controls(&mut self) {
        for key in self.key_voices.iter_mut() {
            key.data.process_controls(&self.voice_control_data);
        }
    }

    fn kill_voices_in_exclusive_class(&mut self, class: u8) {
        for key in self.key_voices.iter_mut() {
            key.data.kill_by_exclusive_class(class);
        }
    }

    /// Sends a ChannelEvent to the channel.
    /// See the `ChannelEvent` documentation for more information.
    pub fn process_event(&mut self, event: ChannelEvent) {
        self.push_events_iter(std::iter::once(event));
    }

    /// Sends multiple ChannelEvent items to the channel as an iterator.
    pub fn push_events_iter<T: Iterator<Item = ChannelEvent>>(&mut self, iter: T) {
        for e in iter {
            match e {
                ChannelEvent::Audio(audio) => match audio {
                    ChannelAudioEvent::NoteOn { key, vel } => {
                        if key > 127 {
                            continue;
                        }
                        let classes: Vec<_> = self
                            .params
                            .channel_sf
                            .exclusive_classes_attack(key, vel)
                            .collect();
                        for class in classes {
                            self.kill_voices_in_exclusive_class(class);
                        }
                        let last_note = self.control_event_data.last_note;
                        if self.control_event_data.mono {
                            if let Some(last) = last_note.filter(|last| *last != key) {
                                self.key_voices[last as usize]
                                    .event_cache
                                    .push(KeyNoteEvent::AllOff);
                            }
                        }
                        let ev = match self.take_portamento(key) {
                            Some((from_key, time)) => KeyNoteEvent::OnGlide {
                                vel,
                                from_key,
                                time,
                            },
                            None => KeyNoteEvent::On(vel),
                        };
                        self.key_voices[key as usize].event_cache.push(ev);
                    }
                    ChannelAudioEvent::NoteOff { key } => {
                        if let Some(key) = self.key_voices.get_mut(key as usize) {
                            let ev = KeyNoteEvent::Off;
                            key.event_cache.push(ev);
                        }
                    }
                    ChannelAudioEvent::AllNotesOff => {
                        for key in self.key_voices.iter_mut() {
                            let ev = KeyNoteEvent::AllOff;
                            key.event_cache.push(ev);
                        }
                    }
                    ChannelAudioEvent::AllNotesKilled => {
                        for key in self.key_voices.iter_mut() {
                            let ev = KeyNoteEvent::AllKilled;
                            key.event_cache.push(ev);
                        }
                    }
                    ChannelAudioEvent::ResetControl => {
                        self.reset_control();
                    }
                    ChannelAudioEvent::Control(control) => {
                        self.process_control_event(control);
                    }
                    ChannelAudioEvent::ProgramChange(preset) => {
                        self.params.set_preset(preset);
                    }
                    ChannelAudioEvent::SystemReset => {
                        for key in self.key_voices.iter_mut() {
                            key.event_cache.clear();
                            key.event_cache.push(KeyNoteEvent::AllKilled);
                        }
                        self.reset_control();
                        self.reset_program();
                    }
                },
                ChannelEvent::Config(config) => {
                    if self.params.process_config_event(config) {
                        self.apply_default_controllers();
                    }
                }
            }
        }
    }

    /// Returns a reader for the VoiceChannel statistics.
    /// See the `VoiceChannelStatsReader` documentation for more information.
    pub fn get_channel_stats(&self) -> VoiceChannelStatsReader {
        let stats = self.params.stats.clone();
        VoiceChannelStatsReader::new(stats)
    }
}

impl AudioPipe for VoiceChannel {
    fn stream_params(&self) -> &AudioStreamParams {
        &self.params.constant.stream_params
    }

    fn read_samples_unchecked(&mut self, out: &mut [f32]) {
        self.push_key_events_and_render(out);
    }
}
