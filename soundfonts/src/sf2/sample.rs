use super::Sf2ParseError;
use crate::resample::resample_vecs;
use soundfont::data::{hydra::sample::SampleHeader, sample_data::SampleData};
use std::{fs::File, sync::Arc};

#[derive(Clone, Debug)]
pub struct Sf2Sample {
    pub data: Arc<[Arc<[f32]>]>,
    pub loop_start: u32,
    pub loop_end: u32,
    pub sample_rate: u32,
    pub origpitch: u8,
    pub pitchadj: i8,
}

impl Sf2Sample {
    pub fn parse_sf2_samples(
        file: &mut File,
        headers: Vec<SampleHeader>,
        data: SampleData,
        sample_rate: u32,
    ) -> Result<Vec<Self>, Sf2ParseError> {
        let smpl = if let Some(data) = data.smpl {
            data.read_contents(file)
                .map_err(|_| Sf2ParseError::FailedToParseFile)?
        } else {
            return Err(Sf2ParseError::FailedToParseFile);
        };

        let mut samples = Vec::new();

        if let Some(sm24) = data.sm24 {
            // SF2 is 24-bit
            let extra = sm24
                .read_contents(file)
                .map_err(|_| Sf2ParseError::FailedToParseFile)?;
            if smpl.len() / 2 != extra.len() {
                return Err(Sf2ParseError::FailedToParseFile);
            }

            for i in 0..extra.len() {
                let n0 = 0;
                let n1 = extra[i];
                let n2 = smpl[i * 2];
                let n3 = smpl[i * 2 + 1];
                let sample = i32::from_le_bytes([n0, n1, n2, n3]);
                let conv = sample as f32 / i32::MAX as f32;
                samples.push(conv);
            }
        } else {
            // SF2 is 16-bit
            for i in smpl.chunks(2) {
                let n0 = i[0];
                let n1 = i[1];
                let sample = i16::from_le_bytes([n0, n1]);
                let conv = sample as f32 / i16::MAX as f32;
                samples.push(conv);
            }
        }

        let mut out: Vec<Sf2Sample> = Vec::new();

        for h in headers {
            let start = h.start;
            let end = h.end;
            let sample: Vec<f32> = samples[start as usize..end as usize].into();

            let new = Sf2Sample {
                data: if h.sample_rate != sample_rate || !sample.is_empty() {
                    resample_vecs(vec![sample], h.sample_rate as f32, sample_rate as f32)
                } else {
                    Arc::new([sample.into()])
                },
                loop_start: h.loop_start - start,
                loop_end: h.loop_end - start,
                sample_rate: h.sample_rate,
                origpitch: h.origpitch,
                pitchadj: h.pitchadj,
            };
            out.push(new)
        }

        Ok(out)
    }
}
