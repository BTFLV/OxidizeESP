//! Persistent application settings stored as JSON in the platform's
//! configuration directory.

use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Error, Result, fsutil, model::validate_baud, monitor::LineEnding};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Appearance {
    #[default]
    Dark,
    Light,
    System,
}

/// How a serial port is preselected when the application starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PortSelection {
    /// Select the port used last time, if it is present.
    #[default]
    RememberLast,
    /// Select the port automatically when exactly one likely ESP port exists.
    AutoSelectSingle,
    /// Never preselect a port.
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct MonitorSettings {
    pub baud: u32,
    /// Number of lines kept in the monitor buffer.
    pub max_lines: u32,
    pub show_timestamps: bool,
    pub autoscroll: bool,
    pub line_ending: LineEnding,
    pub reset_on_start: bool,
    /// Reopen the monitor when the same device is plugged in again.
    pub auto_reconnect: bool,
    /// Resolve code addresses using the selected firmware ELF.
    pub decode_addresses: bool,
}

impl Default for MonitorSettings {
    fn default() -> Self {
        Self {
            baud: 115_200,
            max_lines: 10_000,
            show_timestamps: true,
            autoscroll: true,
            line_ending: LineEnding::Lf,
            reset_on_start: false,
            auto_reconnect: true,
            decode_addresses: true,
        }
    }
}

pub const MIN_MONITOR_LINES: u32 = 1_000;
pub const MAX_MONITOR_LINES: u32 = 200_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct Settings {
    pub appearance: Appearance,
    /// Baud rate for flashing, backups and other bootloader operations.
    pub flash_baud: u32,
    pub port_selection: PortSelection,
    pub last_port: Option<String>,
    pub firmware_dir: Option<String>,
    pub backup_dir: Option<String>,
    pub use_stub: bool,
    pub verify_after_flash: bool,
    pub show_advanced: bool,
    pub log_level: LogLevel,
    pub monitor: MonitorSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            appearance: Appearance::Dark,
            flash_baud: 460_800,
            port_selection: PortSelection::RememberLast,
            last_port: None,
            firmware_dir: None,
            backup_dir: None,
            use_stub: true,
            verify_after_flash: true,
            show_advanced: false,
            log_level: LogLevel::Info,
            monitor: MonitorSettings::default(),
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        validate_baud(self.flash_baud)?;
        validate_baud(self.monitor.baud)?;
        if !(MIN_MONITOR_LINES..=MAX_MONITOR_LINES).contains(&self.monitor.max_lines) {
            return Err(Error::invalid_input(format!(
                "The monitor buffer must hold between {MIN_MONITOR_LINES} and {MAX_MONITOR_LINES} lines."
            )));
        }
        for dir in [&self.firmware_dir, &self.backup_dir].into_iter().flatten() {
            if !Path::new(dir).is_absolute() {
                return Err(Error::invalid_input("Default folders must be absolute paths."));
            }
        }
        if self.last_port.as_ref().is_some_and(|p| p.len() > 512) {
            return Err(Error::invalid_input("The port name is too long."));
        }
        Ok(())
    }
}

/// Thread-safe settings store backed by a JSON file.
#[derive(Debug)]
pub struct SettingsStore {
    path: Option<PathBuf>,
    current: Mutex<Settings>,
}

impl SettingsStore {
    /// Load settings from `path`. Missing or unreadable files yield the
    /// defaults; the second value describes why the stored file was ignored.
    pub fn load(path: PathBuf) -> (Self, Option<String>) {
        let (settings, warning) = match std::fs::read(&path) {
            Ok(data) => match serde_json::from_slice::<Settings>(&data) {
                Ok(s) if s.validate().is_ok() => (s, None),
                Ok(_) => (Settings::default(), Some("Stored settings were invalid and have been reset.".to_owned())),
                Err(e) => (Settings::default(), Some(format!("Stored settings could not be read: {e}"))),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Settings::default(), None),
            Err(e) => (Settings::default(), Some(format!("Stored settings could not be read: {e}"))),
        };
        (Self { path: Some(path), current: Mutex::new(settings) }, warning)
    }

    /// In-memory store, used when no configuration directory is available.
    pub fn in_memory() -> Self {
        Self { path: None, current: Mutex::new(Settings::default()) }
    }

    pub fn get(&self) -> Settings {
        self.current.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn update(&self, settings: Settings) -> Result<Settings> {
        settings.validate()?;
        if let Some(path) = &self.path {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| Error::file_io(&e, parent, "create"))?;
            }
            let json = serde_json::to_vec_pretty(&settings)
                .map_err(|e| Error::internal("Settings could not be serialized.").with_detail(e.to_string()))?;
            fsutil::write_atomic(path, &json)?;
        }
        *self.current.lock().unwrap_or_else(|p| p.into_inner()) = settings.clone();
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        Settings::default().validate().unwrap();
    }

    #[test]
    fn rejects_invalid_values() {
        let mut s = Settings { flash_baud: 1, ..Settings::default() };
        assert!(s.validate().is_err());
        s.flash_baud = 115_200;
        s.monitor.max_lines = 10;
        assert!(s.validate().is_err());
        s.monitor.max_lines = 5_000;
        s.backup_dir = Some("relative/dir".into());
        assert!(s.validate().is_err());
    }

    #[test]
    fn persists_and_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config").join("settings.json");
        let (store, warning) = SettingsStore::load(path.clone());
        assert!(warning.is_none());
        let mut s = store.get();
        s.appearance = Appearance::Light;
        s.monitor.max_lines = 20_000;
        store.update(s.clone()).unwrap();

        let (reloaded, _) = SettingsStore::load(path);
        assert_eq!(reloaded.get(), s);
    }

    #[test]
    fn tolerates_partial_and_corrupt_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, br#"{"appearance":"light"}"#).unwrap();
        let (store, warning) = SettingsStore::load(path.clone());
        assert!(warning.is_none());
        assert_eq!(store.get().appearance, Appearance::Light);
        assert_eq!(store.get().flash_baud, Settings::default().flash_baud);

        std::fs::write(&path, b"{garbage").unwrap();
        let (store, warning) = SettingsStore::load(path);
        assert!(warning.is_some());
        assert_eq!(store.get(), Settings::default());
    }

    #[test]
    fn invalid_update_is_rejected_and_not_applied() {
        let store = SettingsStore::in_memory();
        let bad = Settings { flash_baud: 0, ..Settings::default() };
        assert!(store.update(bad).is_err());
        assert_eq!(store.get(), Settings::default());
    }
}
