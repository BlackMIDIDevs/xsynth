use super::voice::Voice;
use crate::{helpers::FREQS, AudioStreamParams};

pub trait VoiceSpawner: Sync + Send {
    fn spawn_voice(&self) -> Box<dyn Voice>;
}

pub trait SoundfontBase: Sync + Send {
    fn stream_params<'a>(&'a self) -> &'a AudioStreamParams;

    fn get_voice_spawners_at(&self, key: u8, vel: u8) -> Vec<Box<dyn VoiceSpawner>>;
}

pub struct SineVoice {
    freq: f64,

    amp: f32,
    phase: f64,
}

impl SineVoice {
    pub fn spawn(key: u8, vel: u8, sample_rate: u32) -> Self {
        let freq = (FREQS[key as usize] as f64 / sample_rate as f64) * std::f64::consts::PI;
        let amp = 1.04f32.powf(vel as f32 - 127.0);

        Self {
            freq,
            amp,
            phase: 0.0,
        }
    }
}

impl Voice for SineVoice {
    fn is_ended(&self) -> bool {
        self.amp == 0.0
    }

    fn is_releasing(&self) -> bool {
        self.is_ended()
    }

    fn signal_release(&mut self) {
        self.amp = 0.0;
    }

    fn render_to(&mut self, out: &mut [f32]) {
        for i in 0..out.len() {
            let sample = self.amp * self.phase.cos() as f32;
            self.phase += self.freq;
            out[i] += sample;
        }
    }
}

struct SineVoiceSpawner {
    sample_rate: u32,
    key: u8,
    vel: u8,
}

impl VoiceSpawner for SineVoiceSpawner {
    fn spawn_voice(&self) -> Box<dyn Voice> {
        Box::new(SineVoice::spawn(self.key, self.vel, self.sample_rate))
    }
}

pub struct SineSoundfont {
    stream_params: AudioStreamParams,
}

impl SineSoundfont {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            stream_params: AudioStreamParams::new(sample_rate, channels),
        }
    }
}

impl SoundfontBase for SineSoundfont {
    fn get_voice_spawners_at(&self, key: u8, vel: u8) -> Vec<Box<dyn VoiceSpawner>> {
        vec![Box::new(SineVoiceSpawner {
            sample_rate: self.stream_params.sample_rate,
            key,
            vel,
        })]
    }

    fn stream_params<'a>(&'a self) -> &'a AudioStreamParams {
        &self.stream_params
    }
}
