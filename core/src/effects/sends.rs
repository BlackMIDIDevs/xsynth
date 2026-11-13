//! Shared reverb and chorus effects fed by per-channel sends (CC91 / CC93).
//!
//! The effects only run while a channel sends signal to them, plus the time
//! needed for their tail to decay, so a MIDI that never uses CC91/CC93 does
//! not pay for them.

use crate::AudioStreamParams;

/// Seconds of silence after which an effect stops processing.
const EFFECT_TAIL_SECONDS: f32 = 4.0;

struct Comb {
    buffer: Vec<f32>,
    index: usize,
    filter_store: f32,
}

impl Comb {
    fn new(len: usize) -> Self {
        Self {
            buffer: vec![0.0; len.max(1)],
            index: 0,
            filter_store: 0.0,
        }
    }

    #[inline(always)]
    fn process(&mut self, input: f32, feedback: f32, damp: f32) -> f32 {
        let output = self.buffer[self.index];
        self.filter_store = output * (1.0 - damp) + self.filter_store * damp;
        self.buffer[self.index] = input + self.filter_store * feedback;
        self.index += 1;
        if self.index == self.buffer.len() {
            self.index = 0;
        }
        output
    }

    fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.filter_store = 0.0;
    }
}

struct AllPass {
    buffer: Vec<f32>,
    index: usize,
}

impl AllPass {
    fn new(len: usize) -> Self {
        Self {
            buffer: vec![0.0; len.max(1)],
            index: 0,
        }
    }

    #[inline(always)]
    fn process(&mut self, input: f32) -> f32 {
        let buffered = self.buffer[self.index];
        self.buffer[self.index] = input + buffered * 0.5;
        self.index += 1;
        if self.index == self.buffer.len() {
            self.index = 0;
        }
        buffered - input
    }

    fn clear(&mut self) {
        self.buffer.fill(0.0);
    }
}

// Freeverb tunings, in samples at 44.1kHz
const COMB_TUNING: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_TUNING: [usize; 4] = [556, 441, 341, 225];
const STEREO_SPREAD: usize = 23;

/// A Schroeder/Moorer reverb based on the public domain Freeverb design.
pub struct Reverb {
    combs: [Vec<Comb>; 2],
    allpasses: [Vec<AllPass>; 2],
    feedback: f32,
    damp: f32,
    input_gain: f32,
}

impl Reverb {
    pub fn new(sample_rate: u32) -> Self {
        let scale = sample_rate as f32 / 44_100.0;
        let len = |samples: usize| (samples as f32 * scale) as usize;
        let side = |spread: usize| {
            (
                COMB_TUNING
                    .iter()
                    .map(|&t| Comb::new(len(t + spread)))
                    .collect::<Vec<_>>(),
                ALLPASS_TUNING
                    .iter()
                    .map(|&t| AllPass::new(len(t + spread)))
                    .collect::<Vec<_>>(),
            )
        };
        let (combs_l, allpasses_l) = side(0);
        let (combs_r, allpasses_r) = side(STEREO_SPREAD);

        Self {
            combs: [combs_l, combs_r],
            allpasses: [allpasses_l, allpasses_r],
            feedback: 0.84,
            damp: 0.2,
            input_gain: 0.015,
        }
    }

    /// Processes a mono input sample into a stereo output.
    #[inline(always)]
    fn process(&mut self, input: f32) -> (f32, f32) {
        let input = input * self.input_gain;
        let mut out = [0.0f32; 2];
        for (side, out) in out.iter_mut().enumerate() {
            let mut acc = 0.0;
            for comb in self.combs[side].iter_mut() {
                acc += comb.process(input, self.feedback, self.damp);
            }
            for allpass in self.allpasses[side].iter_mut() {
                acc = allpass.process(acc);
            }
            *out = acc;
        }
        (out[0], out[1])
    }

    fn clear(&mut self) {
        for side in 0..2 {
            self.combs[side].iter_mut().for_each(Comb::clear);
            self.allpasses[side].iter_mut().for_each(AllPass::clear);
        }
    }
}

/// A stereo chorus made of modulated delay lines.
pub struct Chorus {
    buffer: [Vec<f32>; 2],
    index: usize,
    phase: f32,
    phase_step: f32,
    base_delay: f32,
    depth: f32,
}

impl Chorus {
    pub fn new(sample_rate: u32) -> Self {
        let sample_rate = sample_rate as f32;
        let len = (sample_rate * 0.05) as usize + 2;
        Self {
            buffer: [vec![0.0; len], vec![0.0; len]],
            index: 0,
            phase: 0.0,
            phase_step: 0.6 / sample_rate,
            base_delay: sample_rate * 0.012,
            depth: sample_rate * 0.003,
        }
    }

    #[inline(always)]
    fn read(&self, side: usize, delay: f32) -> f32 {
        let buffer = &self.buffer[side];
        let len = buffer.len();
        let pos = self.index as f32 - delay;
        let pos = if pos < 0.0 { pos + len as f32 } else { pos };
        let i = pos as usize % len;
        let frac = pos.fract();
        let next = (i + 1) % len;
        buffer[i] + (buffer[next] - buffer[i]) * frac
    }

    #[inline(always)]
    fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        self.buffer[0][self.index] = left;
        self.buffer[1][self.index] = right;

        let angle = self.phase * std::f32::consts::TAU;
        let delay_l = self.base_delay + self.depth * angle.sin();
        let delay_r = self.base_delay + self.depth * angle.cos();
        let out = (self.read(0, delay_l), self.read(1, delay_r));

        self.phase = (self.phase + self.phase_step).fract();
        self.index += 1;
        if self.index == self.buffer[0].len() {
            self.index = 0;
        }
        out
    }

    fn clear(&mut self) {
        self.buffer[0].fill(0.0);
        self.buffer[1].fill(0.0);
    }
}

/// Activity tracking that lets an effect stop once its tail is silent.
struct EffectState {
    idle_frames: usize,
    tail_frames: usize,
    active: bool,
}

impl EffectState {
    fn new(sample_rate: u32) -> Self {
        Self {
            idle_frames: 0,
            tail_frames: (sample_rate as f32 * EFFECT_TAIL_SECONDS) as usize,
            active: false,
        }
    }

    /// Returns true if the effect should process this block.
    fn update(&mut self, has_input: bool, frames: usize) -> bool {
        if has_input {
            self.active = true;
            self.idle_frames = 0;
        } else if self.active {
            self.idle_frames += frames;
            if self.idle_frames > self.tail_frames {
                self.active = false;
            }
        }
        self.active
    }
}

/// Reverb and chorus send buses shared by all channels of a synthesizer.
///
/// Usage per rendered block: call [`SendEffects::begin`], then
/// [`SendEffects::add_channel`] with each channel's dry output and send levels,
/// then [`SendEffects::finish`] to mix the effect returns into the output.
pub struct SendEffects {
    stream_params: AudioStreamParams,
    reverb: Reverb,
    chorus: Chorus,
    reverb_state: EffectState,
    chorus_state: EffectState,
    reverb_bus: Vec<f32>,
    chorus_bus: Vec<f32>,
    reverb_input: bool,
    chorus_input: bool,
    /// Send levels of each channel in the previous block, for smoothing
    last_sends: Vec<(f32, f32)>,
}

impl SendEffects {
    pub fn new(stream_params: AudioStreamParams) -> Self {
        let sample_rate = stream_params.sample_rate;
        Self {
            stream_params,
            reverb: Reverb::new(sample_rate),
            chorus: Chorus::new(sample_rate),
            reverb_state: EffectState::new(sample_rate),
            chorus_state: EffectState::new(sample_rate),
            reverb_bus: Vec::new(),
            chorus_bus: Vec::new(),
            reverb_input: false,
            chorus_input: false,
            last_sends: Vec::new(),
        }
    }

    /// Starts a new block of `len` interleaved samples.
    pub fn begin(&mut self, len: usize) {
        self.reverb_input = false;
        self.chorus_input = false;
        if self.reverb_state.active {
            prepare(&mut self.reverb_bus, len);
        }
        if self.chorus_state.active {
            prepare(&mut self.chorus_bus, len);
        }
    }

    /// Adds a channel's dry output to the send buses. Send levels are ramped
    /// from the previous block's values to avoid clicks.
    pub fn add_channel(&mut self, channel: usize, samples: &[f32], sends: (f32, f32)) {
        if self.last_sends.len() <= channel {
            self.last_sends.resize(channel + 1, (0.0, 0.0));
        }
        let last = self.last_sends[channel];
        self.last_sends[channel] = sends;

        if last.0 > 0.0 || sends.0 > 0.0 {
            if !self.reverb_input {
                self.reverb_input = true;
                if !self.reverb_state.active {
                    prepare(&mut self.reverb_bus, samples.len());
                }
            }
            add_ramped(&mut self.reverb_bus, samples, last.0, sends.0);
        }
        if last.1 > 0.0 || sends.1 > 0.0 {
            if !self.chorus_input {
                self.chorus_input = true;
                if !self.chorus_state.active {
                    prepare(&mut self.chorus_bus, samples.len());
                }
            }
            add_ramped(&mut self.chorus_bus, samples, last.1, sends.1);
        }
    }

    /// Processes the effects and adds their output to `out`.
    pub fn finish(&mut self, out: &mut [f32]) {
        let channels = self.stream_params.channels.count() as usize;
        let frames = out.len() / channels;

        if self.reverb_state.update(self.reverb_input, frames) {
            if self.reverb_bus.len() != out.len() {
                prepare(&mut self.reverb_bus, out.len());
            }
            for (o, i) in out
                .chunks_exact_mut(channels)
                .zip(self.reverb_bus.chunks_exact(channels))
            {
                let input = i.iter().sum::<f32>();
                let (l, r) = self.reverb.process(input);
                if channels == 1 {
                    o[0] += (l + r) * 0.5;
                } else {
                    o[0] += l;
                    o[1] += r;
                }
            }
        } else if self.reverb_state.idle_frames > 0 {
            self.reverb.clear();
            self.reverb_state.idle_frames = 0;
        }

        if self.chorus_state.update(self.chorus_input, frames) {
            if self.chorus_bus.len() != out.len() {
                prepare(&mut self.chorus_bus, out.len());
            }
            for (o, i) in out
                .chunks_exact_mut(channels)
                .zip(self.chorus_bus.chunks_exact(channels))
            {
                if channels == 1 {
                    let (l, r) = self.chorus.process(i[0], i[0]);
                    o[0] += (l + r) * 0.5;
                } else {
                    let (l, r) = self.chorus.process(i[0], i[1]);
                    o[0] += l;
                    o[1] += r;
                }
            }
        } else if self.chorus_state.idle_frames > 0 {
            self.chorus.clear();
            self.chorus_state.idle_frames = 0;
        }
    }

    /// Whether any effect is currently processing.
    pub fn is_active(&self) -> bool {
        self.reverb_state.active || self.chorus_state.active
    }
}

fn prepare(buf: &mut Vec<f32>, len: usize) {
    buf.clear();
    buf.resize(len, 0.0);
}

fn add_ramped(bus: &mut [f32], samples: &[f32], from: f32, to: f32) {
    if from == to {
        for (b, s) in bus.iter_mut().zip(samples) {
            *b += s * to;
        }
    } else {
        let step = (to - from) / samples.len().max(1) as f32;
        let mut gain = from;
        for (b, s) in bus.iter_mut().zip(samples) {
            gain += step;
            *b += s * gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChannelCount;

    fn params() -> AudioStreamParams {
        AudioStreamParams::new(48_000, ChannelCount::Stereo)
    }

    #[test]
    fn silent_sends_do_not_process() {
        let mut fx = SendEffects::new(params());
        let dry = vec![0.5; 512];
        let mut out = vec![0.0; 512];
        fx.begin(out.len());
        fx.add_channel(0, &dry, (0.0, 0.0));
        fx.finish(&mut out);
        assert!(!fx.is_active());
        assert!(out.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn reverb_send_produces_a_tail() {
        let mut fx = SendEffects::new(params());
        let mut impulse = vec![0.0; 4096];
        impulse[0] = 1.0;
        impulse[1] = 1.0;
        let mut out = vec![0.0; 4096];
        fx.begin(out.len());
        fx.add_channel(0, &impulse, (1.0, 1.0));
        fx.finish(&mut out);
        assert!(fx.is_active());

        // The tail keeps ringing after the send stops
        let silence = vec![0.0; 4096];
        let mut out = vec![0.0; 4096];
        fx.begin(out.len());
        fx.add_channel(0, &silence, (0.0, 0.0));
        fx.finish(&mut out);
        assert!(out.iter().any(|s| s.abs() > 1e-6));
        assert!(fx.is_active());
    }
}
