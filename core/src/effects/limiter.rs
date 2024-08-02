use std::marker::PhantomData;

struct SingleChannelLimiter {
    loudness: f32,
    attack: f32,
    falloff: f32,
    strength: f32,
    min_thresh: f32,
}

impl SingleChannelLimiter {
    fn new() -> SingleChannelLimiter {
        SingleChannelLimiter {
            loudness: 1.0,
            attack: 100.0,
            falloff: 16000.0,
            strength: 1.0,
            min_thresh: 1.0,
        }
    }

    fn limit(&mut self, val: f32) -> f32 {
        let abs = val.abs();
        if self.loudness > abs {
            self.loudness = (self.loudness * self.falloff + abs) / (self.falloff + 1.0);
        } else {
            self.loudness = (self.loudness * self.attack + abs) / (self.attack + 1.0);
        }

        if self.loudness < self.min_thresh {
            self.loudness = self.min_thresh;
        }

        let val = val / (self.loudness * self.strength + 2.0 * (1.0 - self.strength)) / 2.0;

        val
    }
}

/// A multi-channel audio limiter.
///
/// Can be useful to prevent clipping on loud audio.
pub struct VolumeLimiter {
    channels: Vec<SingleChannelLimiter>,
    channel_count: usize,
}

pub struct VolumeLimiterIter<'a, 'b, T: 'b + Iterator<Item = f32>> {
    limiter: &'a mut VolumeLimiter,
    samples: T,
    pos: usize,
    _b: PhantomData<&'b T>,
}

impl VolumeLimiter {
    /// Initializes a new audio limiter with a specified audio channel count.
    pub fn new(channel_count: u16) -> VolumeLimiter {
        let mut limiters = Vec::new();
        for _ in 0..channel_count {
            limiters.push(SingleChannelLimiter::new());
        }
        VolumeLimiter {
            channels: limiters,
            channel_count: channel_count as usize,
        }
    }

    /// Applies the limiting algorithm to the given sample buffer to prevent clipping.
    pub fn limit(&mut self, sample: &mut [f32]) {
        for (i, s) in sample.iter_mut().enumerate() {
            *s = self.channels[i % self.channel_count].limit(*s);
        }
    }

    pub fn limit_iter<'a, 'b, T: 'b + Iterator<Item = f32>>(
        &'a mut self,
        samples: T,
    ) -> VolumeLimiterIter<'a, 'b, T> {
        impl<'a, 'b, T: 'b + Iterator<Item = f32>> Iterator for VolumeLimiterIter<'a, 'b, T> {
            type Item = f32;

            fn next(&mut self) -> Option<Self::Item> {
                let next = self.samples.next();
                if let Some(next) = next {
                    let val =
                        self.limiter.channels[self.pos % self.limiter.channel_count].limit(next);
                    self.pos += 1;
                    Some(val)
                } else {
                    None
                }
            }
        }

        VolumeLimiterIter::<'a, 'b, T> {
            _b: PhantomData,
            limiter: self,
            samples,
            pos: 0,
        }
    }
}
