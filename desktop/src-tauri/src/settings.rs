//! Persisted user settings for the launcher UI: theme preference and the
//! auto-update toggle that decides whether npx installs the latest dsh build
//! or reuses its cache. Stored as JSON in the app config directory.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// How the launcher's own UI should look.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    /// Follow the operating system appearance.
    System,
    Dark,
    Light,
}

/// User-facing settings, persisted to `settings.json`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// When true, npx resolves `@deepseek-ai/dsh@latest` on every launch and
    /// installs a newer build when one exists; when false, npx reuses its
    /// cached version without checking for updates.
    pub auto_update: bool,
    /// Theme for the launcher's own windows (splash + settings).
    pub theme: ThemePreference,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_update: true,
            theme: ThemePreference::System,
        }
    }
}

/// Owns the on-disk settings file and the in-memory copy. The `Mutex` keeps
/// concurrent readers (UI commands, the startup thread) consistent.
pub struct SettingsStore {
    path: PathBuf,
    inner: Mutex<Settings>,
}

impl SettingsStore {
    /// Load settings from the app config dir, falling back to defaults (and
    /// writing them back) when the file is missing or unreadable.
    pub fn load_or_default(app: &AppHandle) -> Self {
        let path = app
            .path()
            .app_config_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("settings.json");
        let settings = fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        let store = Self {
            path,
            inner: Mutex::new(settings),
        };
        let _ = store.persist();
        store
    }

    /// Current settings snapshot.
    pub fn get(&self) -> Settings {
        self.inner.lock().expect("settings lock").clone()
    }

    /// Replace the settings and persist them. Returns an error only when the
    /// file cannot be written; the in-memory value is still updated.
    pub fn set(&self, settings: Settings) -> Result<(), String> {
        *self.inner.lock().expect("settings lock") = settings;
        self.persist()
    }

    fn persist(&self) -> Result<(), String> {
        let settings = self.inner.lock().expect("settings lock").clone();
        let json = serde_json::to_string_pretty(&settings).map_err(|error| error.to_string())?;
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        }
        fs::write(&self.path, json).map_err(|error| error.to_string())
    }
}
