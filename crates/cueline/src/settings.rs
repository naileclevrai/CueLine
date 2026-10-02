//! Per-machine preferences (audio device, recent files), stored in
//! `%APPDATA%\CueLine\settings.json`. Project files stay portable.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::engine::device::AudioConfig;

const MAX_RECENT: usize = 10;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub audio: AudioConfig,
    pub recent: Vec<PathBuf>,
    pub follow_playhead: bool,
    pub snap_to_frames: bool,
    /// Stop the transport when the playhead passes the last clip or marker.
    pub stop_at_end: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            audio: AudioConfig::default(),
            recent: Vec::new(),
            follow_playhead: true,
            snap_to_frames: true,
            stop_at_end: false,
        }
    }
}

fn settings_path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("CueLine").join("settings.json"))
}

impl Settings {
    pub fn load() -> Self {
        settings_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        // Automated captures must never touch the user's real preferences.
        if std::env::var_os("CUELINE_SCREENSHOT").is_some() {
            return;
        }
        let Some(path) = settings_path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    log::warn!("cannot save settings: {e}");
                }
            }
            Err(e) => log::warn!("cannot serialize settings: {e}"),
        }
    }

    pub fn push_recent(&mut self, path: PathBuf) {
        self.recent.retain(|p| p != &path);
        self.recent.insert(0, path);
        self.recent.truncate(MAX_RECENT);
    }
}
