use super::Sf2ParseError;
use riff::Chunk;
use std::fs::File;

pub struct Sdta {
    pub smpl: Vec<u8>,
    pub sm24: Option<Vec<u8>>,
}

impl Sdta {
    pub fn new(chunks: &Chunk, file: &mut File) -> Result<Self, Sf2ParseError> {
        if chunks.id().as_str() != "LIST"
            || chunks
                .read_type(file)
                .map_err(|_| Sf2ParseError::FailedToParseFile)?
                .as_str()
                != "sdta"
        {
            return Err(Sf2ParseError::CorruptChunks);
        }

        let mut smpl_ch: Option<Chunk> = None;
        let mut sm24_ch: Option<Chunk> = None;

        for chunk in chunks.iter(file) {
            let chunk = chunk.map_err(|_| Sf2ParseError::FailedToParseFile)?;
            match chunk.id().as_str() {
                "smpl" => {
                    smpl_ch = Some(chunk);
                }
                "sm24" => {
                    sm24_ch = Some(chunk);
                }
                _ => {
                    return Err(Sf2ParseError::CorruptChunks);
                }
            }
        }

        let mut smpl = Vec::new();

        if let Some(chunk) = smpl_ch {
            let data = chunk
                .read_contents(file)
                .map_err(|_| Sf2ParseError::FailedToParseFile)?;
            smpl.extend_from_slice(&data[..]);
        }

        let mut sm24 = None;

        if let Some(chunk) = sm24_ch {
            let data = chunk
                .read_contents(file)
                .map_err(|_| Sf2ParseError::FailedToParseFile)?;
            sm24 = Some(data);
        }

        Ok(Self { smpl, sm24 })
    }
}
