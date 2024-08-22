use crate::utils::*;
use clap::{command, Arg, ArgAction};
use std::path::PathBuf;
use xsynth_core::{
    channel::ChannelInitOptions,
    channel_group::{ChannelGroupConfig, ParallelismOptions, ThreadCount},
    soundfont::{Interpolator, SoundfontInitOptions},
    AudioStreamParams, ChannelCount,
};

#[derive(Clone, Debug, PartialEq)]
pub struct XSynthRenderConfig {
    pub group_options: ChannelGroupConfig,

    pub sf_options: SoundfontInitOptions,

    pub use_limiter: bool,
}

#[derive(Clone, Debug)]
pub struct State {
    pub config: XSynthRenderConfig,
    pub layers: Option<usize>,
    pub midi: PathBuf,
    pub soundfonts: Vec<PathBuf>,
    pub output: PathBuf,
}

impl State {
    const THREADING_HELP: &'static str = "Use \"none\" for no multithreading, \"auto\" for multithreading with\n\
                              an automatically determined thread count or any number to specify the\n\
                              amount of threads that should be used.\n\
                              Default: \"auto\"";

    pub fn from_args() -> Self {
        let matches = command!()
            .args([
                Arg::new("midi")
                    .required(true)
                    .help("The MIDI file to be converted."),
                Arg::new("soundfonts")
                    .required(true)
                    .help("The list of soundfonts. Will be loaded in the order they are typed.")
                    .action(ArgAction::Append),
                Arg::new("output").short('o').long("output").help(
                    "The path of the output audio file.\n\
                    Default: \"out.wav\"",
                ),
                Arg::new("sample rate")
                    .short('s')
                    .help(
                        "The sample rate of the output audio in Hz.\n\
                        Default: 48000 (48kHz)",
                    )
                    .value_parser(int_parser),
                Arg::new("audio channels")
                    .short('c')
                    .help(
                        "The audio channel count of the output audio.\n\
                        Supported: \"1\" (mono) and \"2\" (stereo)\n\
                        Default: stereo",
                    )
                    .value_parser(audio_channels_parser),
                Arg::new("layer limit")
                    .short('l')
                    .long("layers")
                    .help(
                        "The layer limit for each channel. Use \"0\" for unlimited layers.\n\
                        One layer is one voice per key per channel.\n\
                        Default: 32",
                    )
                    .value_parser(layers_parser),
                Arg::new("channel threading")
                    .long("channel_threading")
                    .help("Per-channel multithreading options.\n".to_owned() + Self::THREADING_HELP)
                    .value_parser(threading_parser),
                Arg::new("key threading")
                    .long("key_threading")
                    .help("Per-key multithreading options.\n".to_owned() + Self::THREADING_HELP)
                    .value_parser(threading_parser),
                Arg::new("limiter")
                    .short('L')
                    .long("apply_limiter")
                    .help("Apply audio limiter to the output audio to prevent clipping.")
                    .action(ArgAction::SetTrue),
                Arg::new("disable fade out voice killing")
                    .long("disable_fade_out")
                    .help("Disables fade out when killing a voice. This may cause popping.")
                    .action(ArgAction::SetFalse),
                Arg::new("linear release")
                    .long("linear_release")
                    .help("Use linear release phase in the volume envelope.")
                    .action(ArgAction::SetTrue),
                Arg::new("interpolation")
                    .short('I')
                    .long("interpolation")
                    .help(
                        "The interpolation algorithm to use. Available options are\n\
                        \"none\" (no interpolation) and \"linear\" (linear interpolation).",
                    )
                    .value_parser(interpolation_parser),
            ])
            .get_matches();

        let midi = matches
            .get_one::<String>("midi")
            .cloned()
            .unwrap_or_default();

        let output = matches
            .get_one::<String>("output")
            .map(|s| {
                if s.is_empty() {
                    String::from("out.wav")
                } else {
                    s.clone()
                }
            })
            .unwrap_or_default();

        let soundfonts = matches
            .get_many::<String>("soundfonts")
            .unwrap_or_default()
            .map(PathBuf::from)
            .collect::<Vec<_>>();

        let config = XSynthRenderConfig {
            group_options: ChannelGroupConfig {
                channel_init_options: ChannelInitOptions {
                    fade_out_killing: matches
                        .get_one("disable fade out voice killing")
                        .copied()
                        .unwrap_or(true),
                    drums_only: false,
                },
                channel_count: 16,
                drums_channels: vec![9],
                audio_params: AudioStreamParams::new(
                    matches.get_one("sample rate").copied().unwrap_or(48000),
                    matches
                        .get_one("audio channels")
                        .copied()
                        .unwrap_or(ChannelCount::Stereo),
                ),
                parallelism: ParallelismOptions {
                    channel: matches
                        .get_one("channel threading")
                        .copied()
                        .unwrap_or(ThreadCount::Auto),
                    key: matches
                        .get_one("key threading")
                        .copied()
                        .unwrap_or(ThreadCount::Auto),
                },
            },
            sf_options: SoundfontInitOptions {
                bank: None,
                preset: None,
                linear_release: matches
                    .get_one("linear release")
                    .copied()
                    .unwrap_or_default(),
                use_effects: true,
                interpolator: matches
                    .get_one("interpolation")
                    .copied()
                    .unwrap_or(Interpolator::Linear),
            },
            use_limiter: matches.get_one("limiter").copied().unwrap_or_default(),
        };

        Self {
            config,
            layers: matches.get_one("layer limit").copied().unwrap_or(Some(32)),
            midi: PathBuf::from(midi),
            output: PathBuf::from(output),
            soundfonts,
        }
    }
}
