//! Flash address arithmetic and region validation.
//!
//! Every address and length received from the frontend passes through this
//! module before it reaches the hardware layer.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Error, Result};

/// Smallest erasable unit of SPI flash.
pub const SECTOR_SIZE: u32 = 0x1000;
/// Largest addressable flash supported by espflash (256 MiB).
pub const MAX_FLASH_SIZE: u32 = 0x1000_0000;

/// A contiguous range of flash memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FlashRegion {
    pub offset: u32,
    pub length: u32,
}

/// Alignment requirement for a region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    /// No alignment requirement.
    None,
    /// Offset and length must be multiples of 4 bytes.
    Word,
    /// Offset and length must be multiples of the 4 KiB sector size.
    Sector,
}

impl FlashRegion {
    pub fn new(offset: u32, length: u32) -> Self {
        Self { offset, length }
    }

    /// Exclusive end address, or `None` if it would overflow.
    pub fn end(&self) -> Option<u32> {
        self.offset.checked_add(self.length)
    }

    pub fn overlaps(&self, other: &FlashRegion) -> bool {
        let (Some(a_end), Some(b_end)) = (self.end(), other.end()) else {
            return true;
        };
        self.offset < b_end && other.offset < a_end
    }

    /// Validate the region against alignment rules and an optional flash
    /// capacity.
    pub fn validate(&self, alignment: Alignment, flash_size: Option<u32>) -> Result<()> {
        if self.length == 0 {
            return Err(Error::invalid_input("The length must be greater than zero."));
        }
        let end =
            self.end().ok_or_else(|| Error::invalid_input("The region extends beyond the 32-bit address space."))?;
        if end > MAX_FLASH_SIZE {
            return Err(Error::invalid_input(format!(
                "The region ends at {}, beyond the maximum supported flash size of 256 MiB.",
                format_address(end)
            )));
        }
        if let Some(size) = flash_size
            && end > size
        {
            return Err(Error::invalid_input(format!(
                "The region {}–{} exceeds the flash size of {}.",
                format_address(self.offset),
                format_address(end),
                format_size(u64::from(size))
            )));
        }
        match alignment {
            Alignment::None => {}
            Alignment::Word => {
                if !self.offset.is_multiple_of(4) {
                    return Err(Error::invalid_input(format!(
                        "The offset {} must be a multiple of 4 bytes.",
                        format_address(self.offset)
                    )));
                }
            }
            Alignment::Sector => {
                if !self.offset.is_multiple_of(SECTOR_SIZE) {
                    return Err(Error::invalid_input(format!(
                        "The offset {} must be aligned to the 4 KiB sector size (0x1000).",
                        format_address(self.offset)
                    )));
                }
                if !self.length.is_multiple_of(SECTOR_SIZE) {
                    return Err(Error::invalid_input(format!(
                        "The length {} must be a multiple of the 4 KiB sector size (0x1000).",
                        format_address(self.length)
                    )));
                }
            }
        }
        Ok(())
    }

    /// Split the region into consecutive chunks of at most `chunk` bytes.
    pub fn chunks(&self, chunk: u32) -> impl Iterator<Item = FlashRegion> + use<> {
        let chunk = chunk.max(1);
        let start = self.offset;
        let end = self.offset.saturating_add(self.length);
        let mut cursor = start;
        std::iter::from_fn(move || {
            if cursor >= end {
                return None;
            }
            let len = (end - cursor).min(chunk);
            let region = FlashRegion::new(cursor, len);
            cursor += len;
            Some(region)
        })
    }
}

/// Parse an address or length entered by a user.
///
/// Accepts hexadecimal (`0x1000`), decimal (`4096`), underscores as digit
/// separators and size suffixes `K`/`KB`/`KiB` and `M`/`MB`/`MiB` (binary
/// multiples, as is conventional for flash sizes).
pub fn parse_u32(input: &str) -> Result<u32> {
    let cleaned: String = input.trim().chars().filter(|c| *c != '_').collect();
    if cleaned.is_empty() {
        return Err(Error::invalid_input("A value is required."));
    }
    let invalid = || Error::invalid_input(format!("\"{}\" is not a valid number.", input.trim()));

    if let Some(hex) = cleaned.strip_prefix("0x").or_else(|| cleaned.strip_prefix("0X")) {
        if hex.is_empty() {
            return Err(invalid());
        }
        return u32::from_str_radix(hex, 16).map_err(|_| invalid());
    }

    let lower = cleaned.to_ascii_lowercase();
    let (digits, multiplier) = if let Some(d) = strip_any(&lower, &["kib", "kb", "k"]) {
        (d, 1024u64)
    } else if let Some(d) = strip_any(&lower, &["mib", "mb", "m"]) {
        (d, 1024 * 1024)
    } else {
        (lower.as_str(), 1)
    };
    let digits = digits.trim();
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(invalid());
    }
    let value: u64 = digits.parse().map_err(|_| invalid())?;
    let value = value
        .checked_mul(multiplier)
        .filter(|v| *v <= u64::from(u32::MAX))
        .ok_or_else(|| Error::invalid_input(format!("\"{}\" is too large.", input.trim())))?;
    u32::try_from(value).map_err(|_| invalid())
}

fn strip_any<'a>(value: &'a str, suffixes: &[&str]) -> Option<&'a str> {
    suffixes.iter().find_map(|s| value.strip_suffix(s))
}

pub fn format_address(value: u32) -> String {
    format!("0x{value:08X}")
}

pub fn format_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * 1024;
    if bytes >= MIB && bytes.is_multiple_of(MIB / 16) {
        format!("{} MiB", trim_float(bytes as f64 / MIB as f64))
    } else if bytes >= KIB {
        format!("{} KiB", trim_float(bytes as f64 / KIB as f64))
    } else {
        format!("{bytes} B")
    }
}

fn trim_float(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_numbers() {
        assert_eq!(parse_u32("0x1000").unwrap(), 0x1000);
        assert_eq!(parse_u32("0X10_000").unwrap(), 0x10000);
        assert_eq!(parse_u32("4096").unwrap(), 4096);
        assert_eq!(parse_u32(" 64K ").unwrap(), 65536);
        assert_eq!(parse_u32("4MiB").unwrap(), 4 * 1024 * 1024);
        assert_eq!(parse_u32("2mb").unwrap(), 2 * 1024 * 1024);
        assert_eq!(parse_u32("0xFFFFFFFF").unwrap(), u32::MAX);
    }

    #[test]
    fn rejects_invalid_numbers() {
        for input in ["", "0x", "0xZZ", "-1", "1.5", "abc", "0x100000000", "5000M", "k"] {
            assert!(parse_u32(input).is_err(), "{input} should be rejected");
        }
    }

    #[test]
    fn validates_alignment() {
        assert!(FlashRegion::new(0x1000, 0x2000).validate(Alignment::Sector, None).is_ok());
        assert!(FlashRegion::new(0x1001, 0x2000).validate(Alignment::Sector, None).is_err());
        assert!(FlashRegion::new(0x1000, 0x2001).validate(Alignment::Sector, None).is_err());
        assert!(FlashRegion::new(0x1004, 0x3).validate(Alignment::Word, None).is_ok());
        assert!(FlashRegion::new(0x1002, 0x4).validate(Alignment::Word, None).is_err());
    }

    #[test]
    fn rejects_empty_and_overflowing_regions() {
        assert!(FlashRegion::new(0, 0).validate(Alignment::None, None).is_err());
        assert!(FlashRegion::new(u32::MAX, 2).validate(Alignment::None, None).is_err());
        assert!(FlashRegion::new(0x0FFF_F000, 0x2000).validate(Alignment::None, None).is_err());
    }

    #[test]
    fn validates_against_flash_size() {
        let size = 4 * 1024 * 1024;
        assert!(FlashRegion::new(0, size).validate(Alignment::Sector, Some(size)).is_ok());
        assert!(FlashRegion::new(0x1000, size).validate(Alignment::Sector, Some(size)).is_err());
    }

    #[test]
    fn detects_overlap() {
        let a = FlashRegion::new(0x1000, 0x1000);
        assert!(a.overlaps(&FlashRegion::new(0x1800, 0x1000)));
        assert!(!a.overlaps(&FlashRegion::new(0x2000, 0x1000)));
        assert!(!a.overlaps(&FlashRegion::new(0x0, 0x1000)));
    }

    #[test]
    fn chunks_cover_region_exactly() {
        let region = FlashRegion::new(0x1000, 0x2_5000);
        let chunks: Vec<_> = region.chunks(0x1_0000).collect();
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], FlashRegion::new(0x1000, 0x1_0000));
        assert_eq!(chunks[2], FlashRegion::new(0x2_1000, 0x5000));
        let total: u32 = chunks.iter().map(|c| c.length).sum();
        assert_eq!(total, region.length);
    }

    #[test]
    fn formats_sizes() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(4096), "4 KiB");
        assert_eq!(format_size(4 * 1024 * 1024), "4 MiB");
        assert_eq!(format_address(0x8000), "0x00008000");
    }
}
