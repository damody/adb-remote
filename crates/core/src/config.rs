use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceClientConfig {
    pub server_url: Option<String>,
    pub adb_path: Option<PathBuf>,
    pub agent_id: Option<String>,
    pub devices: Vec<SharedDeviceConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedDeviceConfig {
    pub adb_serial: String,
    pub display_name: String,
    pub password: Option<String>,
    pub group: Option<String>,
    pub tags: Vec<String>,
}

impl SharedDeviceConfig {
    pub fn password_required(&self) -> bool {
        self.password.is_some()
    }
}

pub fn config_dir(app: &str) -> Result<PathBuf> {
    let dirs = ProjectDirs::from("dev", "adb-remote", app)
        .context("could not resolve application config directory")?;
    let dir = dirs.config_dir().to_path_buf();
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn device_config_path() -> Result<PathBuf> {
    Ok(config_dir("adb-remote-client")?.join("config.toml"))
}

pub fn load_device_config() -> Result<DeviceClientConfig> {
    let path = device_config_path()?;
    if !path.exists() {
        return Ok(DeviceClientConfig::default());
    }
    let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

pub fn save_device_config(config: &DeviceClientConfig) -> Result<()> {
    let path = device_config_path()?;
    let text = toml::to_string_pretty(config)?;
    fs::write(&path, text).with_context(|| format!("write {}", path.display()))
}
