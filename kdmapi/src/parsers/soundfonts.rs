use super::ConfigPath;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
use xsynth_core::{
    soundfont::{SampleSoundfont, SoundfontBase, SoundfontInitOptions},
    AudioStreamParams,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SFDescriptor {
    pub path: PathBuf,
    pub enabled: bool,
    pub options: SoundfontInitOptions,
}

impl Default for SFDescriptor {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            enabled: true,
            options: Default::default(),
        }
    }
}

impl SFDescriptor {
    pub fn path(&self) -> Option<PathBuf> {
        let path = PathBuf::from(&self.path);
        if path.exists() && self.enabled {
            Some(path)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SFList {
    soundfonts: Vec<SFDescriptor>,
}

impl Default for SFList {
    fn default() -> Self {
        Self {
            soundfonts: vec![SFDescriptor::default()],
        }
    }
}

impl SFList {
    pub fn create_sfbase_vector(
        self,
        stream_params: AudioStreamParams,
    ) -> Vec<Arc<dyn SoundfontBase>> {
        let mut out: Vec<Arc<dyn SoundfontBase>> = Vec::new();
        for sf in self.soundfonts {
            if let Some(path) = sf.path() {
                match SampleSoundfont::new(path, stream_params, sf.options) {
                    Ok(sf) => out.push(Arc::new(sf)),
                    Err(e) => println!("Error loading soundfont: {e}"),
                }
            }
        }
        out
    }
}

impl ConfigPath for SFList {
    fn filename() -> PathBuf {
        "soundfonts.json".into()
    }
}
