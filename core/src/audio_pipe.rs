use crate::AudioStreamParams;

pub trait AudioPipe {
    /// The stream parameters of the audio pipe
    fn stream_params<'a>(&'a self) -> &'a AudioStreamParams;

    /// Reads samples from the pipe
    fn read_samples(&mut self, to: &mut [f32]) {
        assert!(to.len() as u32 % self.stream_params().channels as u32 == 0);
        self.read_samples_unchecked(to);
    }

    /// Reads samples from the pipe without checking the channel count of the output
    fn read_samples_unchecked(&mut self, to: &mut [f32]);
}

pub struct FunctionAudioPipe<F: 'static + FnMut(&mut [f32]) + Send> {
    func: F,
    stream_params: AudioStreamParams,
}

impl<F: 'static + FnMut(&mut [f32]) + Send> AudioPipe for FunctionAudioPipe<F> {
    fn stream_params<'a>(&'a self) -> &'a AudioStreamParams {
        &self.stream_params
    }

    fn read_samples_unchecked(&mut self, to: &mut [f32]) {
        (self.func)(to);
    }
}

impl<F: 'static + FnMut(&mut [f32]) + Send> FunctionAudioPipe<F> {
    pub fn new(sample_rate: u32, channels: u16, func: F) -> Self {
        FunctionAudioPipe {
            func,
            stream_params: AudioStreamParams::new(sample_rate, channels),
        }
    }
}
