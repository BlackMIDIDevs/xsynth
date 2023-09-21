use std::path::PathBuf;

use thiserror::Error;

mod sfbk;

#[derive(Error, Debug, Clone)]
pub enum Sf2ParseError {
    #[error("Failed to read file: {0}")]
    FailedToReadFile(PathBuf),

    #[error("Corrupt chunks")]
    CorruptChunks,

    #[error("Failed to parse file")]
    FailedToParseFile,
}

pub fn load_soundfont(sf2_path: impl Into<PathBuf>) -> Result<(), Sf2ParseError> {
    let _sfbk = sfbk::Sfbk::new(sf2_path)?;
    Ok(())
}
