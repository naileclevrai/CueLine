//! Typography.
//!
//! CueLine prefers Apple's San Francisco family when it is installed on the
//! machine: it is read from the system at runtime and never bundled, since
//! its licence forbids redistribution. Otherwise the embedded Inter
//! (SIL Open Font License, see `assets/fonts/OFL.txt`) is used.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontId};

pub const TEXT: &str = "sf-text";
pub const TEXT_MEDIUM: &str = "sf-text-medium";
pub const TEXT_SEMIBOLD: &str = "sf-text-semibold";
pub const DISPLAY: &str = "sf-display";
pub const DISPLAY_LIGHT: &str = "sf-display-light";
pub const DISPLAY_MEDIUM: &str = "sf-display-medium";
pub const MONO: &str = "mono";

const INTER_REGULAR: &[u8] = include_bytes!("../../../../assets/fonts/Inter-Regular.ttf");
const INTER_MEDIUM: &[u8] = include_bytes!("../../../../assets/fonts/Inter-Medium.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../../../../assets/fonts/Inter-SemiBold.ttf");
const INTER_DISPLAY_LIGHT: &[u8] = include_bytes!("../../../../assets/fonts/InterDisplay-Light.ttf");
const INTER_DISPLAY_MEDIUM: &[u8] = include_bytes!("../../../../assets/fonts/InterDisplay-Medium.ttf");

/// (family, system font files in order of preference, embedded fallback)
const FACES: &[(&str, &[&str], Option<&[u8]>)] = &[
    (TEXT, &["SF-Pro-Text-Regular.otf", "SF-Pro-Display-Regular.otf"], Some(INTER_REGULAR)),
    (TEXT_MEDIUM, &["SF-Pro-Text-Medium.otf", "SF-Pro-Display-Medium.otf"], Some(INTER_MEDIUM)),
    (TEXT_SEMIBOLD, &["SF-Pro-Text-Semibold.otf", "SF-Pro-Display-Semibold.otf"], Some(INTER_SEMIBOLD)),
    (DISPLAY, &["SF-Pro-Display-Regular.otf", "SF-Pro-Text-Regular.otf"], Some(INTER_REGULAR)),
    (DISPLAY_LIGHT, &["SF-Pro-Display-Light.otf", "SF-Pro-Display-Thin.otf"], Some(INTER_DISPLAY_LIGHT)),
    (DISPLAY_MEDIUM, &["SF-Pro-Display-Medium.otf", "SF-Pro-Display-Semibold.otf"], Some(INTER_DISPLAY_MEDIUM)),
    (MONO, &["SF-Mono-Regular.otf", "SFMono-Regular.otf", "CascadiaMono.ttf", "consola.ttf"], None),
];

fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(PathBuf::from(local).join("Microsoft").join("Windows").join("Fonts"));
    }
    if let Some(win) = std::env::var_os("WINDIR") {
        dirs.push(PathBuf::from(win).join("Fonts"));
    }
    dirs.push(PathBuf::from("/Library/Fonts"));
    dirs.push(PathBuf::from("/System/Library/Fonts"));
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/fonts"));
    }
    dirs
}

fn find(names: &[&str], dirs: &[PathBuf]) -> Option<(String, Vec<u8>)> {
    names.iter().find_map(|n| {
        dirs.iter().find_map(|d| std::fs::read(d.join(n)).ok().map(|b| (n.to_string(), b)))
    })
}

/// Installs the fonts and returns whether San Francisco was found.
pub fn install(ctx: &egui::Context) -> bool {
    let dirs = font_dirs();
    let mut defs = FontDefinitions::default();
    let fallback_prop = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let fallback_mono = defs.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    let mut has_sf = false;

    let prefer_system = std::env::var_os("CUELINE_NO_SYSTEM_FONTS").is_none();
    for (family, names, embedded) in FACES {
        let mut chain = Vec::new();
        if let Some((file, bytes)) = find(names, &dirs).filter(|_| prefer_system) {
            has_sf |= file.starts_with("SF");
            defs.font_data.insert(family.to_string(), Arc::new(FontData::from_owned(bytes)));
            chain.push(family.to_string());
        } else if let Some(bytes) = embedded {
            let key = format!("{family}-embedded");
            defs.font_data.insert(key.clone(), Arc::new(FontData::from_static(bytes)));
            chain.push(key);
        }
        // egui's bundled fonts cover symbols/emoji the system font may lack.
        chain.extend(if *family == MONO { fallback_mono.clone() } else { fallback_prop.clone() });
        defs.families.insert(FontFamily::Name((*family).into()), chain);
    }
    let text_chain = defs.families[&FontFamily::Name(TEXT.into())].clone();
    defs.families.insert(FontFamily::Proportional, text_chain);
    let mono_chain = defs.families[&FontFamily::Name(MONO.into())].clone();
    defs.families.insert(FontFamily::Monospace, mono_chain);
    ctx.set_fonts(defs);
    log::info!("typography: {}", if has_sf { "San Francisco (system)" } else { "Inter (embedded)" });
    has_sf
}

pub fn family(name: &'static str) -> FontFamily {
    FontFamily::Name(name.into())
}

pub fn text(size: f32) -> FontId {
    FontId::new(size, family(TEXT))
}
pub fn medium(size: f32) -> FontId {
    FontId::new(size, family(TEXT_MEDIUM))
}
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, family(TEXT_SEMIBOLD))
}
pub fn display(size: f32) -> FontId {
    FontId::new(size, family(DISPLAY))
}
pub fn display_light(size: f32) -> FontId {
    FontId::new(size, family(DISPLAY_LIGHT))
}
pub fn display_medium(size: f32) -> FontId {
    FontId::new(size, family(DISPLAY_MEDIUM))
}
pub fn mono(size: f32) -> FontId {
    FontId::new(size, family(MONO))
}
