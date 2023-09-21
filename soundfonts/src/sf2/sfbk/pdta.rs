use super::Sf2ParseError;
use riff::Chunk;
use std::fs::File;

pub struct Pdta {}

impl Pdta {
    pub fn new(_chunks: &Chunk, _file: &mut File) -> Result<Self, Sf2ParseError> {
        Ok(Self {})
    }
}
