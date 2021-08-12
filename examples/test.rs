use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use xsynth::core::{BufferedRenderer, FunctionAudioPipe};

fn main() {
    let host = cpal::default_host();

    let device = host
        .default_output_device()
        .expect("failed to find output device");
    println!("Output device: {}", device.name().unwrap());

    let config = device.default_output_config().unwrap();
    println!("Default output config: {:?}", config);

    match config.sample_format() {
        cpal::SampleFormat::F32 => run::<f32>(&device, &config.into()),
        cpal::SampleFormat::I16 => run::<i16>(&device, &config.into()),
        cpal::SampleFormat::U16 => run::<u16>(&device, &config.into()),
    }
    .unwrap();
}

pub fn run<T>(device: &cpal::Device, config: &cpal::StreamConfig) -> Result<(), ()>
where
    T: 'static + cpal::Sample + Send + Default,
{
    let sample_rate = config.sample_rate.0 as f32;
    let channels = config.channels as usize;

    // Produce a sinusoid of maximum amplitude.
    let mut sample_clock = 0f32;
    let mut next_value = move || {
        sample_clock = (sample_clock + 1.0) % sample_rate;
        (sample_clock * 440.0 * 2.0 * std::f32::consts::PI / sample_rate).sin()
    };

    let err_fn = |err| eprintln!("an error occurred on stream: {}", err);

    let rate = config.sample_rate.0;

    let reader = FunctionAudioPipe::new(rate, config.channels, move |data: &mut [f32]| {
        write_data(data, channels, &mut next_value);
    });

    let mut buffered = BufferedRenderer::new(reader, rate, 48, config.channels);

    let mut float_data = Vec::new();
    let stream = device
        .build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                float_data.reserve(data.len());
                for _ in 0..data.len() {
                    float_data.push(0.0);
                }
                buffered.read(&mut float_data);
                let mut i = 0;
                for s in float_data.drain(0..) {
                    data[i] = cpal::Sample::from(&s);
                    i += 1;
                }
            },
            err_fn,
        )
        .unwrap();
    stream.play().unwrap();

    std::thread::sleep(std::time::Duration::from_millis(10000000000));

    Ok(())
}

fn write_data(output: &mut [f32], channels: usize, next_sample: &mut dyn FnMut() -> f32)
{
    for frame in output.chunks_mut(channels) {
        let value = next_sample();
        for sample in frame.iter_mut() {
            *sample = value;
        }
    }
}
