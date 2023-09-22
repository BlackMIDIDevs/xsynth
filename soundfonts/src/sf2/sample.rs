use std::{sync::Arc, fs::File};
use soundfont::data::{hydra::sample::SampleHeader, sample_data::SampleData};
use super::Sf2ParseError;

pub fn parse_sf2_samples(file: &mut File, headers: Vec<SampleHeader>, data: SampleData) -> Result<Vec<Arc<[i32]>>, Sf2ParseError> {
    let smpl = if let Some(data) = data.smpl {
        data.read_contents(file).map_err(|_| Sf2ParseError::FailedToParseFile)?
    } else {
        return Err(Sf2ParseError::FailedToParseFile);
    };

    let mut samples = Vec::new();

    if let Some(sm24) = data.sm24 {
        // SF2 is 24-bit
        let extra = sm24.read_contents(file).map_err(|_| Sf2ParseError::FailedToParseFile)?;
        if smpl.len() / 2 != extra.len() {
            return Err(Sf2ParseError::FailedToParseFile);
        }

        for i in 0..extra.len() {
            let n0 = 0i32;
            let n1 = smpl[i*2] as i32;
            let n2 = smpl[i*2+1] as i32;
            let n3 = extra[i] as i32;
            let sample: i32 = (n0 << 24) | (n1 << 16) | ( n2 << 8 ) | (n3);
            samples.push(sample);
        }
    } else {
        // SF2 is 16-bit
        for i in smpl.chunks(2) {
            let n0 = i[0] as i32;
            let n1 = i[1] as i32;
            let sample: i32 = ( n0 << 8 ) | (n1);
            samples.push(sample);
        }
    }

    let mut out: Vec<Arc<[i32]>> = Vec::new();

    for h in headers {
        let start = h.start as usize;
        let end = h.end as usize;
        out.push(samples[start..end].into())
    }

    Ok(out)
}
