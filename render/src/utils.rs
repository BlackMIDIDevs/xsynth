use atomic_float::AtomicF64;
use midi_toolkit::{io::MIDIFile, sequence::event::get_channels_array_statistics};
use std::sync::{atomic::Ordering, Arc};
use xsynth_core::{channel_group::ThreadCount, soundfont::Interpolator, ChannelCount};

#[inline(always)]
pub fn layers_parser(s: &str) -> Result<Option<usize>, String> {
    let l: usize = s.parse().map_err(|e| format!("{}", e))?;
    match l {
        0 => Ok(None),
        layers => Ok(Some(layers)),
    }
}

#[inline(always)]
pub fn threading_parser(s: &str) -> Result<ThreadCount, String> {
    match s {
        "none" => Ok(ThreadCount::None),
        "auto" => Ok(ThreadCount::Auto),
        n => {
            let threads: usize = n.parse().map_err(|e| format!("{}", e))?;
            Ok(ThreadCount::Manual(threads))
        }
    }
}

#[inline(always)]
pub fn audio_channels_parser(s: &str) -> Result<ChannelCount, String> {
    match s {
        "mono" => Ok(ChannelCount::Mono),
        "stereo" => Ok(ChannelCount::Stereo),
        _ => Err("Invalid channel count".to_string()),
    }
}

#[inline(always)]
pub fn int_parser(s: &str) -> Result<u32, String> {
    s.parse().map_err(|e| format!("{}", e))
}

#[inline(always)]
pub fn interpolation_parser(s: &str) -> Result<Interpolator, String> {
    match s {
        "none" => Ok(Interpolator::Nearest),
        "linear" => Ok(Interpolator::Linear),
        _ => Err("Invalid interpolation type".to_string()),
    }
}

pub fn get_midi_length(path: &str) -> f64 {
    let midi = MIDIFile::open(path, None).unwrap();
    let parse_length_outer = Arc::new(AtomicF64::new(f64::NAN));
    let ppq = midi.ppq();
    let tracks = midi.iter_all_tracks().collect();
    let stats = get_channels_array_statistics(tracks);
    if let Ok(stats) = stats {
        parse_length_outer.store(
            stats.calculate_total_duration(ppq).as_secs_f64(),
            Ordering::Relaxed,
        );
    }

    parse_length_outer.load(Ordering::Relaxed)
}
