pub trait AudioPipe {
    /// The sample rate of the audio pipe
    fn sample_rate(&self) -> u32;

    /// The number of stereo channels of the audio pipe
    fn channels(&self) -> u16;

    /// Reads samples from the pipe
    fn read_samples(&mut self, to: &mut [f32]) {
        assert!(to.len() as u32 % self.channels() as u32 == 0);
        self.read_samples_unchecked(to);
    }

    /// Reads samples from the pipe without checking the channel count of the output
    fn read_samples_unchecked(&mut self, to: &mut [f32]);
}

pub struct FunctionAudioPipe<F: 'static + FnMut(&mut [f32]) + Send> {
    func: F,
    sample_rate: u32,
    channels: u16,
}

impl<F: 'static + FnMut(&mut [f32]) + Send> AudioPipe for FunctionAudioPipe<F> {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn read_samples_unchecked(&mut self, to: &mut [f32]) {
        (self.func)(to);
    }
}

impl<F: 'static + FnMut(&mut [f32]) + Send> FunctionAudioPipe<F> {
    pub fn new(sample_rate: u32, channels: u16, func: F) -> Self {
        FunctionAudioPipe {
            func,
            sample_rate,
            channels,
        }
    }
}
