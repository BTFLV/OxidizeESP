//! File helpers: validated input files, atomic output files and hashing.

use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

use crate::{Error, Result};

/// Upper bound for files loaded fully into memory (firmware, ELF files,
/// partition tables). Flash chips supported by espflash are at most 256 MiB.
pub const MAX_INPUT_FILE_SIZE: u64 = 256 * 1024 * 1024;

/// Read an input file after checking that it exists, is a regular file and
/// has a plausible size.
pub fn read_input(path: &Path, max_size: u64) -> Result<Vec<u8>> {
    let metadata = fs::metadata(path).map_err(|e| Error::file_io(&e, path, "open"))?;
    if !metadata.is_file() {
        return Err(Error::invalid_file(format!("\"{}\" is not a regular file.", crate::error::display_name(path))));
    }
    if metadata.len() == 0 {
        return Err(Error::invalid_file(format!("\"{}\" is empty.", crate::error::display_name(path))));
    }
    if metadata.len() > max_size {
        return Err(Error::invalid_file(format!(
            "\"{}\" is too large ({}).",
            crate::error::display_name(path),
            crate::region::format_size(metadata.len())
        )));
    }
    fs::read(path).map_err(|e| Error::file_io(&e, path, "read"))
}

pub fn require_absolute(path: &Path) -> Result<()> {
    if path.is_absolute() { Ok(()) } else { Err(Error::invalid_input("File paths must be absolute.")) }
}

/// Check that a file can be created at `path` without overwriting a
/// directory, and that its parent directory exists.
pub fn validate_output(path: &Path) -> Result<()> {
    require_absolute(path)?;
    if path.is_dir() {
        return Err(Error::invalid_input("The output path refers to a folder, not a file."));
    }
    let parent = path.parent().ok_or_else(|| Error::invalid_input("The output path has no parent folder."))?;
    if !parent.is_dir() {
        return Err(Error::invalid_input(format!(
            "The output folder \"{}\" does not exist.",
            crate::error::display_name(parent)
        )));
    }
    Ok(())
}

/// A file written to a temporary location next to its destination and
/// moved into place only by [`AtomicFile::commit`]. Dropping it without
/// committing deletes the temporary file, so interrupted operations never
/// leave a partial file at the destination.
pub struct AtomicFile {
    temp: NamedTempFile,
    dest: PathBuf,
    hasher: Sha256,
    written: u64,
}

impl AtomicFile {
    pub fn create(dest: &Path) -> Result<Self> {
        validate_output(dest)?;
        let parent = dest.parent().unwrap_or_else(|| Path::new("."));
        let temp = tempfile::Builder::new()
            .prefix(".oxidizeesp-")
            .suffix(".partial")
            .tempfile_in(parent)
            .map_err(|e| Error::file_io(&e, dest, "create"))?;
        Ok(Self { temp, dest: dest.to_path_buf(), hasher: Sha256::new(), written: 0 })
    }

    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        self.temp.write_all(data).map_err(|e| Error::file_io(&e, &self.dest, "write"))?;
        self.hasher.update(data);
        self.written += data.len() as u64;
        Ok(())
    }

    pub fn written(&self) -> u64 {
        self.written
    }

    /// Flush, sync and move the file to its destination. Returns the SHA-256
    /// digest of the content as lowercase hex.
    pub fn commit(mut self) -> Result<String> {
        let dest = self.dest.clone();
        self.temp.flush().map_err(|e| Error::file_io(&e, &dest, "write"))?;
        self.temp.as_file().sync_all().map_err(|e| Error::file_io(&e, &dest, "write"))?;
        let digest = hex(&self.hasher.finalize());
        self.temp.persist(&dest).map_err(|e| Error::file_io(&e.error, &dest, "save"))?;
        Ok(digest)
    }
}

/// Write `data` atomically to `dest`, returning its SHA-256 digest.
pub fn write_atomic(dest: &Path, data: &[u8]) -> Result<String> {
    let mut file = AtomicFile::create(dest)?;
    file.write(data)?;
    file.commit()
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

/// Compute the SHA-256 digest of a file without loading it into memory.
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(|e| Error::file_io(&e, path, "open"))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(Error::file_io(&e, path, "read")),
        };
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_file_is_invisible_until_commit() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out.bin");
        let mut file = AtomicFile::create(&dest).unwrap();
        file.write(b"hello").unwrap();
        assert!(!dest.exists());
        let digest = file.commit().unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"hello");
        assert_eq!(digest, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
        assert_eq!(sha256_file(&dest).unwrap(), digest);
    }

    #[test]
    fn dropped_atomic_file_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out.bin");
        {
            let mut file = AtomicFile::create(&dest).unwrap();
            file.write(b"partial").unwrap();
        }
        assert!(!dest.exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn validates_paths() {
        assert!(validate_output(Path::new("relative.bin")).is_err());
        let dir = tempfile::tempdir().unwrap();
        assert!(validate_output(dir.path()).is_err());
        assert!(validate_output(&dir.path().join("missing").join("x.bin")).is_err());
        assert!(validate_output(&dir.path().join("x.bin")).is_ok());
    }

    #[test]
    fn rejects_empty_and_missing_inputs() {
        let dir = tempfile::tempdir().unwrap();
        let empty = dir.path().join("empty.bin");
        fs::write(&empty, b"").unwrap();
        assert!(read_input(&empty, 10).is_err());
        assert!(read_input(&dir.path().join("missing.bin"), 10).is_err());
        let big = dir.path().join("big.bin");
        fs::write(&big, [0u8; 11]).unwrap();
        assert!(read_input(&big, 10).is_err());
        assert!(read_input(dir.path(), 10).is_err());
    }
}
