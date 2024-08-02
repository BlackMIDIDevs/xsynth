use crate::AudioStreamParams;

/// An object to read audio samples from.
pub trait AudioPipe {
    /// The audio stream parameters of the audio pipe.
    fn stream_params(&self) -> &'_ AudioStreamParams;

    /// Reads samples from the pipe.
    ///
    /// When using in a MIDI synthesizer, the amount of samples determines the
    /// time of the current active MIDI events. For example if we send a note
    /// on event and read 44100 samples (with a 44.1kHz sample rate), then the
    /// note will be audible for 1 second. If after reading those samples we
    /// send a note off event for the same key, then on the next read the key
    /// will be released. If we don't, then the note will keep playing.
    fn read_samples(&mut self, to: &mut [f32]) {
        assert!(to.len() as u32 % self.stream_params().channels.count() as u32 == 0);
        self.read_samples_unchecked(to);
    }

    /// Reads samples from the pipe without checking the channel count of the output.
    fn read_samples_unchecked(&mut self, to: &mut [f32]);
}

pub struct FunctionAudioPipe<F: 'static + FnMut(&mut [f32]) + Send> {
    func: F,
    stream_params: AudioStreamParams,
}

impl<F: 'static + FnMut(&mut [f32]) + Send> AudioPipe for FunctionAudioPipe<F> {
    fn stream_params(&self) -> &'_ AudioStreamParams {
        &self.stream_params
    }

    fn read_samples_unchecked(&mut self, to: &mut [f32]) {
        (self.func)(to);
    }
}

impl<F: 'static + FnMut(&mut [f32]) + Send> FunctionAudioPipe<F> {
    pub fn new(stream_params: AudioStreamParams, func: F) -> Self {
        FunctionAudioPipe {
            func,
            stream_params,
        }
    }
}
