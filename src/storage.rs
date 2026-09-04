use crate::domain::PersistedState;
use directories::ProjectDirs;
use std::{fs, io, path::PathBuf};

#[derive(Debug, Clone, Hash)]
pub struct Paths {
    pub data: PathBuf,
    pub minecraft: PathBuf,
    pub instances: PathBuf,
    pub state_file: PathBuf,
}

impl Paths {
    pub fn discover() -> Self {
        let data = ProjectDirs::from("dev", "AZULC", "AZULC")
            .map(|p| p.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".azulc"));
        Self {
            minecraft: data.join("minecraft"),
            instances: data.join("instances"),
            state_file: data.join("state.json"),
            data,
        }
    }

    pub fn prepare(&self) -> io::Result<()> {
        fs::create_dir_all(&self.minecraft)?;
        fs::create_dir_all(&self.instances)?;
        Ok(())
    }

    pub fn instance_dir(&self, id: uuid::Uuid) -> PathBuf {
        self.instances.join(id.to_string())
    }

    pub fn load(&self) -> PersistedState {
        fs::read_to_string(&self.state_file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, state: &PersistedState) -> io::Result<()> {
        self.prepare()?;
        fs::write(&self.state_file, serde_json::to_vec_pretty(state)?)
    }
}
