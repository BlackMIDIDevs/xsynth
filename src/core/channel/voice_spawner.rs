use crate::core::soundfont::VoiceSpawner;

use super::voice::Voice;

pub struct VoiceSpawnerMatrix {
    voice_spawners_attack: Vec<Vec<Box<dyn VoiceSpawner>>>,
    voice_spawners_release: Vec<Vec<Box<dyn VoiceSpawner>>>,
}

fn voice_iter_from_vec<'a>(vec: &'a Vec<Box<dyn VoiceSpawner>>) -> impl Iterator<Item = Box<dyn Voice>> + 'a {
    vec.iter().map(|voice| voice.spawn_voice())
}

impl VoiceSpawnerMatrix {
    pub fn new() -> Self {
        let mut voice_spawners_attack = Vec::new();
        let mut voice_spawners_release = Vec::new();

        for _ in 0..(128 * 128) {
            voice_spawners_attack.push(Vec::new());
            voice_spawners_release.push(Vec::new());
        }

        voice_spawners_attack.shrink_to_fit();
        voice_spawners_release.shrink_to_fit();

        VoiceSpawnerMatrix {
            voice_spawners_attack,
            voice_spawners_release,
        }
    }

    fn get_spawners_index_at(&self, key: u8, vel: u8) -> usize {
        key as usize + vel as usize * 128
    }

    fn get_attack_spawners_vec_at(&self, key: u8, vel: u8) -> &Vec<Box<dyn VoiceSpawner>> {
        &self.voice_spawners_attack[self.get_spawners_index_at(key, vel)]
    }

    fn get_release_spawners_vec_at(&self, key: u8, vel: u8) -> &Vec<Box<dyn VoiceSpawner>> {
        &self.voice_spawners_release[self.get_spawners_index_at(key, vel)]
    }

    pub fn spawn_voices_attack<'a>(&'a self, key: u8, vel: u8) -> impl Iterator<Item = Box<dyn Voice>> + 'a {
        voice_iter_from_vec(self.get_attack_spawners_vec_at(key, vel))
    }

    pub fn spawn_voices_release<'a>(&'a self, key: u8) -> impl Iterator<Item = Box<dyn Voice>> + 'a {
        voice_iter_from_vec(self.get_release_spawners_vec_at(key, 127))
    }
}
