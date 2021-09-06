use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicI64, AtomicUsize, Ordering},
        Arc, RwLock,
    },
    thread,
    time::{Duration, Instant},
};

use crossbeam_channel::{unbounded, Receiver};

use crate::AudioStreamParams;

use super::AudioPipe;

#[derive(Debug, Clone)]
pub struct BufferedRendererStats {
    /// The number of samples currently buffered.
    /// Can be negative if the reader is waiting for more samples.
    samples: Arc<AtomicI64>,

    /// The number of samples that were in the buffer after the last read.
    last_samples_after_read: Arc<AtomicI64>,

    /// The last number of samples last requested by the read command.
    last_request_samples: Arc<AtomicI64>,

    /// The last 100 render time percentages (0 to 1)
    /// of how long the render thread spent rendering, from the max allowed time.
    render_time: Arc<RwLock<VecDeque<f64>>>,

    /// The number of samples to render each iteration
    render_size: Arc<AtomicUsize>,
}

pub struct BufferedRendererStatsReader {
    stats: BufferedRendererStats,
}

impl BufferedRendererStatsReader {
    pub fn samples(&self) -> i64 {
        self.stats.samples.load(Ordering::Relaxed)
    }

    pub fn last_samples_after_read(&self) -> i64 {
        self.stats.last_samples_after_read.load(Ordering::Relaxed)
    }

    pub fn last_request_samples(&self) -> i64 {
        self.stats.last_request_samples.load(Ordering::Relaxed)
    }

    pub fn render_size(&self) -> usize {
        self.stats.render_size.load(Ordering::Relaxed)
    }

    pub fn average_renderer_load(&self) -> f64 {
        let queue = self.stats.render_time.read().unwrap();
        let total = queue.len();
        queue.iter().sum::<f64>() / total as f64
    }

    pub fn last_renderer_load(&self) -> f64 {
        let queue = self.stats.render_time.read().unwrap();
        *queue.front().unwrap_or(&0.0)
    }
}

/// The helper struct for deferred sample rendering.
/// Helps avoid stutter when the render time is exceding the max time allowed by the audio driver.
/// Instead, it renders in a separate thread with much smaller sample sizes, causing a minimal impact on latency
/// while allowing more time to render per sample.
///
/// Designed to be used in realtime playback only.
pub struct BufferedRenderer {
    stats: BufferedRendererStats,

    /// The receiver for samples (the render thread has the sender).
    receive: Receiver<Vec<f32>>,

    /// Remainder of samples from the last received samples vec.
    remainder: Vec<f32>,

    stream_params: AudioStreamParams,
}

impl BufferedRenderer {
    pub fn new<F: 'static + AudioPipe + Send>(
        mut render: F,
        sample_rate: u32,
        channels: u16,
        render_size: usize,
    ) -> Self {
        let (tx, rx) = unbounded();

        let samples = Arc::new(AtomicI64::new(0));
        let last_request_samples = Arc::new(AtomicI64::new(0));
        let render_size = Arc::new(AtomicUsize::new(render_size));

        let last_samples_after_read = Arc::new(AtomicI64::new(0));

        let render_time = Arc::new(RwLock::new(VecDeque::new()));

        {
            let samples = samples.clone();
            let last_request_samples = last_request_samples.clone();
            let render_size = render_size.clone();
            let render_time = render_time.clone();
            thread::spawn(move || loop {
                let size = render_size.load(Ordering::SeqCst);

                // The expected render time per iteration. It is slightly smaller (*90/100) than
                // the real time so the render thread can catch up if it's behind.
                let delay = Duration::from_secs(1) * size as u32 / sample_rate * 90 / 100;

                // If the render thread is ahead by over ~10%, wait until more samples are required.
                loop {
                    let samples = samples.load(Ordering::SeqCst);
                    let last_requested = last_request_samples.load(Ordering::SeqCst);
                    if samples > last_requested * 110 / 100 {
                        spin_sleep::sleep(delay / 10);
                    } else {
                        break;
                    }
                }

                let start = Instant::now();
                let end = start + delay;

                // Create the vec and write the samples
                let mut vec = vec![Default::default(); size * channels as usize];
                render.read_samples(&mut vec);

                // Send the samples, break if the pipe is broken
                samples.fetch_add(vec.len() as i64, Ordering::SeqCst);
                match tx.send(vec) {
                    Ok(_) => {}
                    Err(_) => break,
                };

                // Write the elapsed render time percentage to the render_time queue
                {
                    let mut queue = render_time.write().unwrap();
                    let elaspsed = start.elapsed().as_secs_f64();
                    let total = delay.as_secs_f64();
                    queue.push_front(elaspsed / total);
                    if queue.len() > 100 {
                        queue.pop_back();
                    }
                }

                // Sleep until the next iteration
                let now = Instant::now();
                if end > now {
                    spin_sleep::sleep(end - now);
                }
            });
        }

        Self {
            stats: BufferedRendererStats {
                samples,
                last_request_samples,
                render_time,
                render_size,
                last_samples_after_read,
            },
            receive: rx,
            remainder: Vec::new(),
            stream_params: AudioStreamParams::new(sample_rate, channels),
        }
    }

    /// Reads samples from the remainder and the output queue into the destination array.
    pub fn read(&mut self, dest: &mut [f32]) {
        let mut i: usize = 0;
        let len = dest.len().min(self.remainder.len());
        let samples = self
            .stats
            .samples
            .fetch_sub(dest.len() as i64, Ordering::SeqCst);

        self.stats
            .last_request_samples
            .store(dest.len() as i64, Ordering::SeqCst);

        // Read from current remainder
        for r in self.remainder.drain(0..len) {
            dest[i] = r;
            i += 1;
        }

        // Read from output queue, leave the remainder if there is any
        while self.remainder.len() == 0 {
            let mut buf = self.receive.recv().unwrap();

            let len = buf.len().min(dest.len() - i);
            for r in buf.drain(0..len) {
                dest[i] = r;
                i += 1;
            }

            self.remainder = buf;
        }

        self.stats
            .last_samples_after_read
            .store(samples, Ordering::Relaxed);
    }

    /// Sets the number of samples that should be rendered each iteration.
    pub fn set_render_size(&self, size: usize) {
        self.stats.render_size.store(size, Ordering::SeqCst);
    }

    pub fn get_buffer_stats(&self) -> BufferedRendererStatsReader {
        BufferedRendererStatsReader {
            stats: self.stats.clone(),
        }
    }
}

impl AudioPipe for BufferedRenderer {
    fn stream_params<'a>(&'a self) -> &'a AudioStreamParams {
        &self.stream_params
    }

    fn read_samples_unchecked(&mut self, to: &mut [f32]) {
        self.read(to)
    }
}
