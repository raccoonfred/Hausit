//! Interface settings that outlive a session: which palettes are open,
//! the interface size and the recently opened projects. Stored as JSON in
//! the user's configuration folder.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UiSettings {
    pub show_properties: bool,
    pub show_browser: bool,
    /// Interface size: 1.0 is 100 %.
    pub scale: f32,
    /// Ribbon shows tabs only until a tab is clicked.
    pub ribbon_minimized: bool,
    /// Most recent first.
    pub recent: Vec<PathBuf>,
    /// Switch to the Modify tab when elements are selected, as Revit does.
    pub contextual_tab_on_select: bool,
}

impl Default for UiSettings {
    fn default() -> Self {
        UiSettings {
            show_properties: true,
            show_browser: true,
            scale: 1.0,
            ribbon_minimized: false,
            recent: Vec::new(),
            contextual_tab_on_select: true,
        }
    }
}

pub const SCALES: [f32; 5] = [0.9, 1.0, 1.15, 1.3, 1.5];

fn config_dir() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if cfg!(windows) {
        env("APPDATA").map(|p| p.join("Hausit"))
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|p| p.join("Library/Application Support/Hausit"))
    } else {
        env("XDG_CONFIG_HOME")
            .or_else(|| env("HOME").map(|h| h.join(".config")))
            .map(|p| p.join("hausit"))
    }
}

fn file() -> Option<PathBuf> {
    config_dir().map(|d| d.join("settings.json"))
}

impl UiSettings {
    pub fn load() -> Self {
        file()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Writes the settings; failures are ignored (settings are a nicety).
    pub fn save(&self) {
        let Some(f) = file() else { return };
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(f, s);
        }
    }

    /// Puts `path` at the top of the recent projects.
    pub fn add_recent(&mut self, path: &Path) {
        let p = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.recent.retain(|r| *r != p);
        self.recent.insert(0, p);
        self.recent.truncate(10);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_moves_to_top_without_duplicates() {
        let mut s = UiSettings::default();
        s.add_recent(Path::new("/nonexistent/a.hausit"));
        s.add_recent(Path::new("/nonexistent/b.hausit"));
        s.add_recent(Path::new("/nonexistent/a.hausit"));
        assert_eq!(s.recent.len(), 2);
        assert!(s.recent[0].ends_with("a.hausit"));
    }

    #[test]
    fn old_settings_files_still_load() {
        let s: UiSettings = serde_json::from_str(r#"{"show_browser": false}"#).unwrap();
        assert!(!s.show_browser);
        assert!(s.show_properties);
    }
}
