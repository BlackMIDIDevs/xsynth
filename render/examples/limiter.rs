struct LookaheadLimiter {
    buffer: Vec<f32>,
    len: usize,
    threshold: f32,
    counter: usize,
    loudness: f32,
    attack: f32,
    falloff: f32,
}

impl LookaheadLimiter {
    pub fn new(threshold: f32, lookahead_samples: usize) -> Self {
        Self {
            buffer: vec![0.0; lookahead_samples],
            len: lookahead_samples,
            threshold,
            counter: 0,
            loudness: 1.0,
            attack: 100.0,
            falloff: 16000.0,
        }
    }

    pub fn process(&mut self, sample: f32) -> f32 {
        let abs = sample.abs();
        self.buffer[self.counter % self.len] = abs;
        self.counter += 1;

        let max_sample = self.buffer.iter().cloned().fold(0.0, f32::max);
        let gain = if max_sample > self.threshold {
            self.threshold / max_sample
        } else {
            self.threshold
        };

        sample * gain
    }
}

use hound;

fn main() {
    let mut reader = hound::WavReader::open("in.wav").unwrap();
    let samples: Vec<f32> = reader.samples().map(|s| s.unwrap()).collect();

    let mut limiterl = LookaheadLimiter::new(0.9, 400);
    let mut limiterr = LookaheadLimiter::new(0.9, 400);

    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 48000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create("out.wav", spec).unwrap();

    for (i, s) in samples.iter().enumerate() {
        if i % 2 == 0 {
            writer.write_sample(limiterr.process(*s)).unwrap();
        } else {
            writer.write_sample(limiterl.process(*s)).unwrap();
        }
    }
    writer.finalize().unwrap();
}