use serde::{Deserialize, Serialize};
use std::{fs::File, io::prelude::*, marker::PhantomData, path::PathBuf};

mod soundfonts;
pub use soundfonts::SFList;

mod settings;
pub use settings::Settings;

const CONFIG_DIR: &str = "xsynth";

pub trait ConfigPath {
    fn filename() -> PathBuf;
}

pub struct Config<T>
where
    T: Default + Serialize + for<'a> Deserialize<'a> + ConfigPath,
{
    path: PathBuf,
    _config: PhantomData<T>,
}

impl<T> Config<T>
where
    T: Default + Serialize + for<'a> Deserialize<'a> + ConfigPath,
{
    pub fn path() -> PathBuf {
        match directories::BaseDirs::new() {
            Some(dirs) => {
                let mut path = dirs.config_dir().to_path_buf();
                path.push(CONFIG_DIR);
                std::fs::create_dir_all(&path).unwrap();
                path.push(T::filename());
                path
            }
            None => PathBuf::from("./"),
        }
    }

    pub fn new() -> Self {
        Self {
            path: Config::<T>::path(),
            _config: PhantomData,
        }
    }

    fn load_from_file(&self) -> Result<T, String> {
        let mut file = File::open(&self.path).map_err(|e| format!("IO error: {e}"))?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .map_err(|e| format!("Loading error: {e}"))?;
        serde_json::from_str(&contents).map_err(|e| format!("Parsing error: {e}"))
    }

    fn save(&self, config: &T) -> Result<(), String> {
        let contents =
            serde_json::to_string_pretty(config).map_err(|e| format!("Parsing error: {e}"))?;
        let mut file = File::create(&self.path).map_err(|e| format!("IO error: {e}"))?;
        file.write_all(contents.as_bytes())
            .map_err(|e| format!("Saving error: {e}"))?;

        Ok(())
    }

    fn create_empty(&self) -> Result<(), String> {
        self.save(&T::default())
    }

    pub fn load(&self) -> Result<T, String> {
        let path = &self.path;
        if !path.exists() {
            self.create_empty()?;
        }
        self.load_from_file()
    }

    pub fn repair(&self) -> Result<(), String> {
        self.save(&self.load()?)
    }
}
