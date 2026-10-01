//! The `.cueline` project file: plain, diff-friendly JSON.

use std::path::{Path, PathBuf};

use cueline_core::{FrameRate, Timecode};
use serde::{Deserialize, Serialize};

pub const EXTENSION: &str = "cueline";
const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub version: u32,
    pub frame_rate: FrameRate,
    #[serde(with = "tc_string")]
    pub start_timecode: Timecode,
    pub user_bits: u32,
    pub master_db: f32,
    pub routing: Routing,
    pub ltc: LtcSettings,
    pub mtc: MtcSettings,
    pub tracks: Vec<TrackDef>,
    pub markers: Vec<Marker>,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            version: VERSION,
            frame_rate: FrameRate::Fps25,
            start_timecode: Timecode::new(1, 0, 0, 0),
            user_bits: 0,
            master_db: 0.0,
            routing: Routing::default(),
            ltc: LtcSettings::default(),
            mtc: MtcSettings::default(),
            tracks: Vec::new(),
            markers: Vec::new(),
        }
    }
}

/// Output channels are 0-based; -1 means "not routed".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Routing {
    pub main_left: i32,
    pub main_right: i32,
}

impl Default for Routing {
    /// Classic two-channel show setup: program on the left, LTC on the right.
    fn default() -> Self {
        Self { main_left: 0, main_right: -1 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LtcSettings {
    pub enabled: bool,
    pub level_db: f32,
    pub channel: i32,
}

impl Default for LtcSettings {
    fn default() -> Self {
        Self { enabled: true, level_db: -12.0, channel: 1 }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MtcSettings {
    pub enabled: bool,
    pub port: Option<String>,
    pub offset_ms: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackDef {
    pub name: String,
    pub path: PathBuf,
    pub gain_db: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    /// Clip start on the timeline, in seconds from the project start.
    pub offset_secs: f64,
    pub color: [u8; 3],
}

impl Default for TrackDef {
    fn default() -> Self {
        Self {
            name: String::new(),
            path: PathBuf::new(),
            gain_db: 0.0,
            pan: 0.0,
            mute: false,
            solo: false,
            offset_secs: 0.0,
            color: [0x4f, 0xa8, 0x8e],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Marker {
    pub name: String,
    pub time_secs: f64,
    pub color: [u8; 3],
}

impl Default for Marker {
    fn default() -> Self {
        Self { name: String::new(), time_secs: 0.0, color: [0xe8, 0xa2, 0x3a] }
    }
}

impl Project {
    pub fn start_frames(&self) -> i64 {
        self.start_timecode.to_frames(self.frame_rate)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let base = path.parent().unwrap_or(Path::new(""));
        let mut copy = self.clone();
        copy.version = VERSION;
        for t in &mut copy.tracks {
            t.path = relative_to(&t.path, base);
        }
        let json = serde_json::to_string_pretty(&copy).map_err(|e| e.to_string())?;
        // Write next to the target then rename, so a crash never leaves a
        // half-written project.
        let tmp = path.with_extension("cueline.tmp");
        std::fs::write(&tmp, json).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("cannot save {}: {e}", path.display()))
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let mut p: Project = serde_json::from_str(&text).map_err(|e| format!("invalid project file: {e}"))?;
        if p.version > VERSION {
            log::warn!("project was saved by a newer CueLine (v{})", p.version);
        }
        let base = path.parent().unwrap_or(Path::new(""));
        for t in &mut p.tracks {
            if t.path.is_relative() {
                t.path = base.join(&t.path);
            }
        }
        if !p.start_timecode.is_valid(p.frame_rate) {
            p.start_timecode = Timecode::default();
        }
        Ok(p)
    }
}

/// Expresses `path` relative to `base` when it lives below it.
fn relative_to(path: &Path, base: &Path) -> PathBuf {
    match path.strip_prefix(base) {
        Ok(rel) if !base.as_os_str().is_empty() => rel.to_path_buf(),
        _ => path.to_path_buf(),
    }
}

mod tc_string {
    use cueline_core::{FrameRate, Timecode};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(tc: &Timecode, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(tc)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Timecode, D::Error> {
        let s = String::deserialize(d)?;
        Timecode::parse(&s, FrameRate::Fps30).ok_or_else(|| serde::de::Error::custom(format!("bad timecode {s:?}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_roundtrip_with_relative_paths() {
        let dir = std::env::temp_dir().join(format!("cueline-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("audio")).unwrap();
        let mut p = Project {
            frame_rate: FrameRate::Fps29_97Df,
            start_timecode: Timecode::new(9, 59, 30, 0),
            ..Default::default()
        };
        p.tracks.push(TrackDef {
            name: "Intro".into(),
            path: dir.join("audio").join("a.wav"),
            offset_secs: 1.5,
            ..Default::default()
        });
        p.markers.push(Marker { name: "Go".into(), time_secs: 12.0, ..Default::default() });
        let file = dir.join("show.cueline");
        p.save(&file).unwrap();

        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("\"09:59:30:00\""));
        assert!(text.contains("\"29.97df\""));
        assert!(!text.contains(&dir.to_string_lossy().replace('\\', "\\\\")), "path should be relative");

        let back = Project::load(&file).unwrap();
        assert_eq!(back, p);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_fields_take_defaults() {
        let p: Project = serde_json::from_str(r#"{"frame_rate":"24"}"#).unwrap();
        assert_eq!(p.frame_rate, FrameRate::Fps24);
        assert!(p.ltc.enabled);
        assert_eq!(p.start_timecode, Timecode::new(1, 0, 0, 0));
    }
}
