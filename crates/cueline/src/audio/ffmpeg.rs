//! Optional bridge to an installed `ffmpeg`, used only for formats CueLine
//! cannot decode or encode by itself (Opus, WMA, AC-3, AAC export, exotic
//! video containers). Nothing is bundled: without ffmpeg, those formats are
//! simply unavailable.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

static OVERRIDE: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Uses `path` instead of searching `PATH` (from the settings).
pub fn set_override(path: Option<PathBuf>) {
    *OVERRIDE.lock().unwrap() = path;
}

fn exe_name() -> &'static str {
    if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }
}

/// Finds ffmpeg: the configured path first, then every directory of `PATH`.
pub fn locate() -> Option<PathBuf> {
    if let Some(p) = OVERRIDE.lock().unwrap().clone() {
        return p.is_file().then_some(p);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(exe_name())).find(|p| p.is_file())
}

fn command(ffmpeg: &Path) -> Command {
    let mut cmd = Command::new(ffmpeg);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// First line of `ffmpeg -version`, e.g. "ffmpeg version 8.1.2".
pub fn version(ffmpeg: &Path) -> Option<String> {
    let out = command(ffmpeg).arg("-version").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().next().map(|l| l.split(" Copyright").next().unwrap_or(l).trim().to_string())
}

/// Runs ffmpeg quietly; on failure returns its last error line.
pub fn run(ffmpeg: &Path, args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let out = command(ffmpeg)
        .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-y"])
        .args(args)
        .output()
        .map_err(|e| format!("cannot run ffmpeg: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(err.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("ffmpeg failed").to_string())
    }
}

/// A file in the temp directory, deleted when dropped.
pub struct TempFile(pub PathBuf);

impl TempFile {
    pub fn new(ext: &str) -> Self {
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self(std::env::temp_dir().join(format!("cueline-{}-{n}.{ext}", std::process::id())))
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Converts any file ffmpeg understands to a float WAV (first audio stream).
pub fn to_wav(input: &Path) -> Result<TempFile, String> {
    let ffmpeg = locate().ok_or("this format needs ffmpeg, which was not found (see Settings → Formats)")?;
    let tmp = TempFile::new("wav");
    run(
        &ffmpeg,
        &["-i".as_ref(), input.as_os_str(), "-vn".as_ref(), "-c:a".as_ref(), "pcm_f32le".as_ref(), tmp.0.as_os_str()],
    )?;
    Ok(tmp)
}
