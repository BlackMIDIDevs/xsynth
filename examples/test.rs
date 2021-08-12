use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use xsynth::core::{
    event::ChannelEvent, AudioPipe, BufferedRenderer, FunctionAudioPipe, VoiceChannel,
};

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
    let err_fn = |err| eprintln!("an error occurred on stream: {}", err);

    let rate = config.sample_rate.0;

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(24)
        .build()
        .unwrap();
    let channel = VoiceChannel::new(rate, config.channels, Some(pool));

    let mut read_channel = channel.clone();
    let reader = FunctionAudioPipe::new(rate, config.channels, move |data: &mut [f32]| {
        read_channel.read_samples(data);
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

    for i in 0..128 {
        for _ in 0..64 {
            channel.process_event(ChannelEvent::NoteOn {
                key: i as u8,
                vel: 64,
            });
        }
    }

    // std::thread::spawn(move || loop {
    //     for i in 0..10 {
    //         channel.process_event(ChannelEvent::NoteOn {
    //             key: (64 + i) as u8,
    //             vel: 127,
    //         });
    //         std::thread::sleep(std::time::Duration::from_millis(1000));
    //         channel.process_event(ChannelEvent::NoteOff {
    //             key: (64 + i) as u8,
    //         });
    //     }
    // });

    std::thread::sleep(std::time::Duration::from_millis(10000000000));

    Ok(())
}

fn write_data(output: &mut [f32], channels: usize, next_sample: &mut dyn FnMut() -> f32) {
    for frame in output.chunks_mut(channels) {
        let value = next_sample();
        for sample in frame.iter_mut() {
            *sample = value;
        }
    }
}
