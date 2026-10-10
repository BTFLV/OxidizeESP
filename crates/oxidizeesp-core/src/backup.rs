//! Flash backup metadata.
//!
//! Each backup `name.bin` is accompanied by a JSON sidecar
//! `name.bin.oxidizeesp.json` describing where the data came from. Backups
//! are plain flash dumps: they do not include eFuses, and data encrypted
//! with a device-specific key cannot be restored to another device.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Error, Result, fsutil, model::Chip};

pub const SIDECAR_SUFFIX: &str = ".oxidizeesp.json";
pub const METADATA_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BackupMetadata {
    pub format_version: u32,
    pub application: String,
    pub application_version: String,
    /// RFC 3339 timestamp of completion.
    pub created_at: String,
    pub chip: Chip,
    pub chip_revision: Option<String>,
    pub mac_address: Option<String>,
    pub usb_serial_number: Option<String>,
    pub port_name: String,
    pub flash_offset: u32,
    pub length: u32,
    /// Detected flash capacity at the time of the backup.
    pub flash_size: Option<u32>,
    /// SHA-256 of the backup file, lowercase hex.
    pub sha256: String,
    /// Whether flash encryption was reported as enabled. Encrypted contents
    /// are only meaningful on the originating device.
    pub flash_encryption: Option<bool>,
    pub full_flash: bool,
}

impl BackupMetadata {
    /// Validate a deserialized sidecar against basic invariants.
    fn validate(&self) -> Result<()> {
        if self.format_version != METADATA_FORMAT_VERSION {
            return Err(Error::invalid_file("The backup metadata uses an unsupported format version."));
        }
        if self.length == 0 || self.flash_offset.checked_add(self.length).is_none() {
            return Err(Error::invalid_file("The backup metadata describes an invalid flash region."));
        }
        if self.sha256.len() != 64 || !self.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Error::invalid_file("The backup metadata contains an invalid checksum."));
        }
        Ok(())
    }
}

pub fn sidecar_path(backup: &Path) -> PathBuf {
    let mut name = backup.as_os_str().to_owned();
    name.push(SIDECAR_SUFFIX);
    PathBuf::from(name)
}

pub fn write_sidecar(backup: &Path, metadata: &BackupMetadata) -> Result<()> {
    let json = serde_json::to_vec_pretty(metadata)
        .map_err(|e| Error::internal("Backup metadata could not be serialized.").with_detail(e.to_string()))?;
    fsutil::write_atomic(&sidecar_path(backup), &json).map(|_| ())
}

/// Read the sidecar for `backup`, if one exists.
pub fn read_sidecar(backup: &Path) -> Result<Option<BackupMetadata>> {
    let path = sidecar_path(backup);
    if !path.is_file() {
        return Ok(None);
    }
    let data = fsutil::read_input(&path, 64 * 1024)?;
    let metadata: BackupMetadata = serde_json::from_slice(&data)
        .map_err(|e| Error::invalid_file("The backup metadata file is malformed.").with_detail(e.to_string()))?;
    metadata.validate()?;
    Ok(Some(metadata))
}

/// Result of verifying a backup file against its metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BackupVerification {
    pub metadata: Option<BackupMetadata>,
    pub actual_sha256: String,
    #[ts(type = "number")]
    pub actual_size: u64,
    /// `None` when there is no metadata to compare against.
    pub matches: Option<bool>,
}

pub fn verify(backup: &Path) -> Result<BackupVerification> {
    fsutil::require_absolute(backup)?;
    let size = std::fs::metadata(backup)
        .map_err(|e| Error::file_io(&e, backup, "open"))?
        .len();
    let actual = fsutil::sha256_file(backup)?;
    let metadata = read_sidecar(backup)?;
    let matches = metadata
        .as_ref()
        .map(|m| m.sha256.eq_ignore_ascii_case(&actual) && u64::from(m.length) == size);
    Ok(BackupVerification {
        metadata,
        actual_sha256: actual,
        actual_size: size,
        matches,
    })
}

/// Suggested file name for a backup, without directory.
pub fn suggested_file_name(chip: Chip, mac: Option<&str>, offset: u32, length: u32, timestamp: &str) -> String {
    let device = mac
        .map(|m| m.replace(':', "").to_ascii_lowercase())
        .unwrap_or_else(|| "device".to_owned());
    let stamp: String = timestamp
        .chars()
        .filter(|c| c.is_ascii_digit())
        .take(14)
        .collect();
    format!(
        "{}-{}-{:#x}-{:#x}-{}.bin",
        chip.to_espflash(),
        device,
        offset,
        length,
        stamp
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn sample(sha: &str) -> BackupMetadata {
        BackupMetadata {
            format_version: METADATA_FORMAT_VERSION,
            application: "OxidizeESP".into(),
            application_version: "0.1.0".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            chip: Chip::Esp32c3,
            chip_revision: Some("0.4".into()),
            mac_address: Some("aa:bb:cc:dd:ee:ff".into()),
            usb_serial_number: None,
            port_name: "COM3".into(),
            flash_offset: 0,
            length: 5,
            flash_size: Some(4 * 1024 * 1024),
            sha256: sha.into(),
            flash_encryption: Some(false),
            full_flash: false,
        }
    }

    #[test]
    fn sidecar_roundtrip_and_verification() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dump.bin");
        let sha = fsutil::write_atomic(&path, b"hello").unwrap();
        write_sidecar(&path, &sample(&sha)).unwrap();

        assert_eq!(sidecar_path(&path).file_name().unwrap(), "dump.bin.oxidizeesp.json");
        let read = read_sidecar(&path).unwrap().unwrap();
        assert_eq!(read.sha256, sha);

        let verification = verify(&path).unwrap();
        assert_eq!(verification.matches, Some(true));

        std::fs::write(&path, b"HELLO").unwrap();
        assert_eq!(verify(&path).unwrap().matches, Some(false));
    }

    #[test]
    fn rejects_invalid_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dump.bin");
        std::fs::write(&path, b"x").unwrap();
        std::fs::write(sidecar_path(&path), b"{not json").unwrap();
        assert!(read_sidecar(&path).is_err());

        let mut bad = sample("zz");
        bad.length = 1;
        std::fs::write(sidecar_path(&path), serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(read_sidecar(&path).is_err());
    }

    #[test]
    fn missing_sidecar_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plain.bin");
        std::fs::write(&path, b"x").unwrap();
        assert!(read_sidecar(&path).unwrap().is_none());
        assert_eq!(verify(&path).unwrap().matches, None);
    }

    #[test]
    fn suggests_descriptive_names() {
        let name = suggested_file_name(Chip::Esp32s3, Some("AA:BB:CC:00:11:22"), 0, 0x40_0000, "2026-03-04T05:06:07Z");
        assert_eq!(name, "esp32s3-aabbcc001122-0x0-0x400000-20260304050607.bin");
    }
}
