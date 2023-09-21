use super::Sf2ParseError;
use std::{fs::File, path::PathBuf};

mod pdta;
mod sdta;
use pdta::Pdta;
use sdta::Sdta;

pub struct Sfbk {
    pub sdta: Sdta,
    pub pdta: Pdta,
}

impl Sfbk {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, Sf2ParseError> {
        let sf2_path: PathBuf = path.into();
        let sf2_path: PathBuf = sf2_path
            .canonicalize()
            .map_err(|_| Sf2ParseError::FailedToReadFile(sf2_path.clone()))?;

        let mut file = File::open(sf2_path.clone())
            .map_err(|_| Sf2ParseError::FailedToReadFile(sf2_path.clone()))?;
        let file = &mut file;
        let riff = riff::Chunk::read(file, 0)
            .map_err(|_| Sf2ParseError::FailedToReadFile(sf2_path.clone()))?;

        if riff.id().as_str() != "RIFF"
            || riff
                .read_type(file)
                .map_err(|_| Sf2ParseError::FailedToReadFile(sf2_path.clone()))?
                .as_str()
                != "sfbk"
        {
            return Err(Sf2ParseError::CorruptChunks);
        }

        let mut sdta = Err(Sf2ParseError::FailedToParseFile);
        let mut pdta = Err(Sf2ParseError::FailedToParseFile);

        let chunks = riff.iter(file).collect::<Vec<_>>();

        for chunk in chunks {
            let chunk = chunk.map_err(|_| Sf2ParseError::FailedToParseFile)?;
            if chunk.id().as_str() != "LIST" {
                return Err(Sf2ParseError::CorruptChunks);
            }

            let ch_type = chunk
                .read_type(file)
                .map_err(|_| Sf2ParseError::CorruptChunks)?;

            match ch_type.as_str() {
                "INFO" => {
                    // We don't need this
                }
                "sdta" => {
                    let data = Sdta::new(&chunk, file)?;
                    sdta = Ok(data);
                }
                "pdta" => {
                    let data = Pdta::new(&chunk, file)?;
                    pdta = Ok(data);
                }
                _ => {
                    return Err(Sf2ParseError::CorruptChunks);
                }
            }
        }

        Ok(Self {
            sdta: sdta?,
            pdta: pdta?,
        })
    }
}
