//! Per-machine settings in `~/.config/tuned-record/config.toml`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const MAX_RECENT: usize = 8;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// Residual latency (frames) measured by the loopback calibration, on top of reported latency.
    #[serde(default)]
    pub calibration_frames: i64,
    #[serde(default)]
    pub recent_projects: Vec<PathBuf>,
    /// Full JACK name of the port to record from, used whenever it is present.
    #[serde(default)]
    pub input_port: Option<String>,
    /// JACK client (device) to play into, used whenever it is present.
    #[serde(default)]
    pub output_device: Option<String>,
}

impl Config {
    fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("tuned-record/config.toml"))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let Some(p) = Self::path() else { return Ok(()) };
        std::fs::create_dir_all(p.parent().unwrap())?;
        std::fs::write(p, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn add_recent(&mut self, dir: PathBuf) {
        self.recent_projects.retain(|d| d != &dir);
        self.recent_projects.insert(0, dir);
        self.recent_projects.truncate(MAX_RECENT);
    }
}
