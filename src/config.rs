use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    pub token: Option<String>,
    #[serde(default = "default_volume_percent")]
    pub volume_percent: u8,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            token: None,
            volume_percent: default_volume_percent(),
        }
    }
}

impl AppConfig {
    pub fn config_path() -> io::Result<PathBuf> {
        let dirs = ProjectDirs::from("", "YaMusicRc", "ya-player").ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "cannot resolve config directory")
        })?;
        Ok(dirs.config_dir().join("config.json"))
    }

    pub fn load() -> io::Result<Self> {
        Self::load_from_path(&Self::config_path()?)
    }

    pub fn save(&self) -> io::Result<()> {
        self.save_to_path(&Self::config_path()?)
    }

    pub fn load_from_path(path: &Path) -> io::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let raw = fs::read_to_string(path)?;
        serde_json::from_str(&raw).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
    }

    pub fn save_to_path(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let raw = serde_json::to_string_pretty(self)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        fs::write(path, raw)
    }

    pub fn redact_token(token: &str) -> String {
        if token.len() <= 8 {
            return "*".repeat(token.len());
        }

        format!("{}...{}", &token[..4], &token[token.len() - 4..])
    }
}

fn default_volume_percent() -> u8 {
    100
}
