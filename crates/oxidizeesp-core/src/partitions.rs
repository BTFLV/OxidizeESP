//! ESP-IDF partition tables: parsing, conversion, validation and layout.
//!
//! Parsing and serialization use the `esp-idf-part` crate, the same
//! implementation espflash relies on.

use std::path::Path;

use esp_idf_part::{AppType, DataType, Flags, Partition, PartitionTable, SubType, Type};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    Error, Result,
    fsutil,
    region::{FlashRegion, format_address, format_size},
};

/// Default offset of the partition table in flash.
pub const DEFAULT_TABLE_OFFSET: u32 = 0x8000;
/// Maximum size of a binary partition table (ESP-IDF reserves 0xC00 bytes).
pub const MAX_TABLE_SIZE: u32 = 0xC00;
/// Size of the flash region reserved for the partition table.
pub const TABLE_REGION_SIZE: u32 = 0x1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TableFormat {
    Csv,
    Binary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PartitionEntry {
    pub name: String,
    /// `app`, `data` or a hexadecimal custom type.
    pub type_name: String,
    pub type_id: u8,
    pub subtype_name: String,
    pub subtype_id: u8,
    pub offset: u32,
    pub size: u32,
    pub encrypted: bool,
    pub readonly: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum IssueSeverity {
    Warning,
    Error,
}

/// A validation finding that does not prevent the table from being parsed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PartitionIssue {
    pub severity: IssueSeverity,
    pub partition: Option<String>,
    pub message: String,
}

/// Parsed partition table prepared for display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PartitionTableView {
    pub entries: Vec<PartitionEntry>,
    pub source_format: TableFormat,
    /// Highest end address of any partition.
    pub end_address: u32,
    pub issues: Vec<PartitionIssue>,
    /// Flash size used to check the layout, if known.
    pub flash_size: Option<u32>,
    pub table_offset: u32,
}

/// Detect whether data looks like a binary partition table.
pub fn detect_format(data: &[u8]) -> TableFormat {
    if data.starts_with(&[0xAA, 0x50]) { TableFormat::Binary } else { TableFormat::Csv }
}

/// Parse a CSV or binary partition table.
pub fn parse(data: &[u8]) -> Result<(PartitionTable, TableFormat)> {
    if data.len() < 2 {
        return Err(Error::invalid_file("The file is too small to be a partition table."));
    }
    let format = detect_format(data);
    // esp-idf-part may panic on some malformed inputs (for example unknown
    // subtype values); treat such panics as invalid input.
    let parsed = match format {
        TableFormat::Binary => {
            let bytes = trim_binary(data.to_vec());
            std::panic::catch_unwind(move || PartitionTable::try_from_bytes(bytes))
        }
        TableFormat::Csv => {
            let text = String::from_utf8(data.to_vec()).map_err(|_| {
                Error::invalid_file("The file is neither a binary partition table nor UTF-8 CSV text.")
            })?;
            std::panic::catch_unwind(move || PartitionTable::try_from_str(text))
        }
    };
    match parsed {
        Ok(Ok(table)) => Ok((table, format)),
        Ok(Err(err)) => Err(err.into()),
        Err(_) => Err(Error::invalid_file("The partition table contains unsupported entries.")),
    }
}

/// Binary tables read from flash are followed by erased (0xFF) bytes. Keep
/// data up to and including the end marker so the parser accepts it, and
/// make sure the length is a multiple of the 32-byte entry size.
fn trim_binary(mut data: Vec<u8>) -> Vec<u8> {
    let entries = data.len() / 32;
    data.truncate(entries * 32);
    data
}

pub fn load(path: &Path) -> Result<(PartitionTable, TableFormat)> {
    let data = fsutil::read_input(path, 1024 * 1024)?;
    parse(&data)
}

pub fn to_entries(table: &PartitionTable) -> Vec<PartitionEntry> {
    table
        .partitions()
        .iter()
        .map(|p| {
            let flags = p.flags();
            PartitionEntry {
                name: p.name(),
                type_name: p.ty().to_string(),
                type_id: u8::from(p.ty()),
                subtype_name: p.subtype().to_string(),
                subtype_id: u8::from(p.subtype()),
                offset: p.offset(),
                size: p.size(),
                encrypted: flags.contains(Flags::ENCRYPTED),
                readonly: flags.contains(Flags::READONLY),
            }
        })
        .collect()
}

/// Build the display model, including layout checks that go beyond the
/// structural validation performed while parsing.
pub fn view(table: &PartitionTable, format: TableFormat, flash_size: Option<u32>, table_offset: u32) -> PartitionTableView {
    let mut entries = to_entries(table);
    entries.sort_by_key(|e| e.offset);
    let mut issues = Vec::new();

    let table_region = FlashRegion::new(table_offset, TABLE_REGION_SIZE);
    for e in &entries {
        let region = FlashRegion::new(e.offset, e.size);
        let end = region.end();
        if end.is_none() {
            issues.push(PartitionIssue {
                severity: IssueSeverity::Error,
                partition: Some(e.name.clone()),
                message: "The partition extends beyond the 32-bit address space.".into(),
            });
            continue;
        }
        if region.overlaps(&table_region) {
            issues.push(PartitionIssue {
                severity: IssueSeverity::Error,
                partition: Some(e.name.clone()),
                message: format!(
                    "Overlaps the partition table at {}.",
                    format_address(table_offset)
                ),
            });
        }
        if e.offset < table_offset {
            issues.push(PartitionIssue {
                severity: IssueSeverity::Error,
                partition: Some(e.name.clone()),
                message: "Starts before the partition table, inside the bootloader area.".into(),
            });
        }
        if let (Some(size), Some(end)) = (flash_size, end)
            && end > size
        {
            issues.push(PartitionIssue {
                severity: IssueSeverity::Error,
                partition: Some(e.name.clone()),
                message: format!("Ends at {}, beyond the {} flash.", format_address(end), format_size(u64::from(size))),
            });
        }
        if e.size == 0 {
            issues.push(PartitionIssue {
                severity: IssueSeverity::Warning,
                partition: Some(e.name.clone()),
                message: "The partition has a size of zero.".into(),
            });
        } else if !e.size.is_multiple_of(0x1000) {
            issues.push(PartitionIssue {
                severity: IssueSeverity::Warning,
                partition: Some(e.name.clone()),
                message: "The size is not a multiple of the 4 KiB flash sector size.".into(),
            });
        }
    }
    let end_address = entries.iter().filter_map(|e| e.offset.checked_add(e.size)).max().unwrap_or(0);

    PartitionTableView {
        entries,
        source_format: format,
        end_address,
        issues,
        flash_size,
        table_offset,
    }
}

pub fn to_binary(table: &PartitionTable) -> Result<Vec<u8>> {
    let bin = table.to_bin().map_err(Error::from)?;
    if bin.len() > MAX_TABLE_SIZE as usize {
        return Err(Error::invalid_file("The partition table exceeds the maximum binary size of 3 KiB."));
    }
    Ok(bin)
}

pub fn to_csv(table: &PartitionTable) -> Result<String> {
    table.to_csv().map_err(Error::from)
}

/// Rebuild a partition table from display entries, for example to export a
/// table that was read from a device. The result is validated.
pub fn from_entries(entries: &[PartitionEntry]) -> Result<PartitionTable> {
    if entries.is_empty() {
        return Err(Error::invalid_input("The partition table has no entries."));
    }
    let mut partitions = Vec::with_capacity(entries.len());
    for e in entries {
        if e.name.is_empty() || e.name.len() > 16 {
            return Err(Error::invalid_input(format!(
                "The partition name \"{}\" must be between 1 and 16 characters long.",
                e.name
            )));
        }
        let ty = Type::from(e.type_id);
        let subtype = match ty {
            Type::App => AppType::from_repr(usize::from(e.subtype_id)).map(SubType::App),
            Type::Data => DataType::from_repr(usize::from(e.subtype_id)).map(SubType::Data),
            Type::Custom(_) => Some(SubType::Custom(e.subtype_id)),
        }
        .ok_or_else(|| {
            Error::invalid_input(format!("Partition \"{}\" has an unknown subtype {:#04x}.", e.name, e.subtype_id))
        })?;
        let mut flags = Flags::empty();
        if e.encrypted {
            flags |= Flags::ENCRYPTED;
        }
        if e.readonly {
            flags |= Flags::READONLY;
        }
        partitions.push(Partition::new(e.name.clone(), ty, subtype, e.offset, e.size, flags));
    }
    let table = PartitionTable::new(partitions);
    table.validate()?;
    Ok(table)
}

/// Resolve partitions by name for erase operations.
pub fn find_by_names(table: &PartitionTable, names: &[String]) -> Result<Vec<PartitionEntry>> {
    let entries = to_entries(table);
    let mut selected = Vec::new();
    for name in names {
        let entry = entries
            .iter()
            .find(|e| &e.name == name)
            .ok_or_else(|| Error::invalid_input(format!("The partition \"{name}\" does not exist.")))?;
        if !selected.iter().any(|e: &PartitionEntry| e.offset == entry.offset) {
            selected.push(entry.clone());
        }
    }
    Ok(selected)
}

/// Whether a partition holds application code.
pub fn is_app(entry: &PartitionEntry) -> bool {
    entry.type_id == u8::from(Type::App)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const SAMPLE_CSV: &str = "\
# Name,   Type, SubType, Offset,  Size, Flags
nvs,      data, nvs,     0x9000,  0x6000,
phy_init, data, phy,     0xf000,  0x1000,
factory,  app,  factory, 0x10000, 1M,
storage,  data, spiffs,  ,        0x40000, readonly
";

    #[test]
    fn parses_csv_and_converts_roundtrip() {
        let (table, format) = parse(SAMPLE_CSV.as_bytes()).unwrap();
        assert_eq!(format, TableFormat::Csv);
        let entries = to_entries(&table);
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[2].name, "factory");
        assert_eq!(entries[2].type_name, "app");
        assert_eq!(entries[2].size, 0x10_0000);
        // Offsets are derived when omitted.
        assert_eq!(entries[3].offset, 0x11_0000);
        assert!(entries[3].readonly);

        let bin = to_binary(&table).unwrap();
        assert_eq!(&bin[..2], &[0xAA, 0x50]);
        let (from_bin, format) = parse(&bin).unwrap();
        assert_eq!(format, TableFormat::Binary);
        assert_eq!(to_entries(&from_bin), entries);

        let csv = to_csv(&from_bin).unwrap();
        let (from_csv, _) = parse(csv.as_bytes()).unwrap();
        assert_eq!(to_entries(&from_csv), entries);
    }

    #[test]
    fn parses_binary_followed_by_erased_flash() {
        let (table, _) = parse(SAMPLE_CSV.as_bytes()).unwrap();
        let mut bin = to_binary(&table).unwrap();
        bin.resize(MAX_TABLE_SIZE as usize, 0xFF);
        let (parsed, _) = parse(&bin).unwrap();
        assert_eq!(to_entries(&parsed).len(), 4);
    }

    #[test]
    fn rejects_overlapping_partitions() {
        let csv = "nvs, data, nvs, 0x9000, 0x8000,\nfactory, app, factory, 0x10000, 0x100000,\n";
        let err = parse(csv.as_bytes()).unwrap_err();
        assert!(err.message.contains("overlap"), "{}", err.message);
    }

    #[test]
    fn rejects_misaligned_app_partition() {
        let csv = "factory, app, factory, 0x11000, 0x100000,\n";
        assert!(parse(csv.as_bytes()).is_err());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse(&[0xAA]).is_err());
        assert!(parse(&[0xAA, 0x50, 1, 2, 3]).is_err());
        assert!(parse(&[0xFF, 0xFE, 0x00, 0x80]).is_err());
        assert!(parse(b"this is not, a partition table").is_err());
    }

    #[test]
    fn view_reports_layout_issues() {
        let (table, format) = parse(SAMPLE_CSV.as_bytes()).unwrap();
        let ok = view(&table, format, Some(4 * 1024 * 1024), DEFAULT_TABLE_OFFSET);
        assert!(ok.issues.is_empty(), "{:?}", ok.issues);
        assert_eq!(ok.end_address, 0x15_0000);

        let small = view(&table, format, Some(1024 * 1024), DEFAULT_TABLE_OFFSET);
        assert!(small.issues.iter().any(|i| i.message.contains("beyond")));

        let shifted = view(&table, format, None, 0x9000);
        assert!(shifted.issues.iter().any(|i| i.message.contains("Overlaps the partition table")));
    }

    #[test]
    fn rebuilds_table_from_entries() {
        let (table, _) = parse(SAMPLE_CSV.as_bytes()).unwrap();
        let entries = to_entries(&table);
        let rebuilt = from_entries(&entries).unwrap();
        assert_eq!(to_binary(&rebuilt).unwrap(), to_binary(&table).unwrap());

        let mut bad = entries.clone();
        bad[0].subtype_id = 0x7F;
        assert!(from_entries(&bad).is_err());
        let mut overlapping = entries;
        overlapping[1].offset = 0x9000;
        assert!(from_entries(&overlapping).is_err());
        assert!(from_entries(&[]).is_err());
    }

    #[test]
    fn finds_partitions_by_name() {
        let (table, _) = parse(SAMPLE_CSV.as_bytes()).unwrap();
        let found = find_by_names(&table, &["nvs".into(), "storage".into(), "nvs".into()]).unwrap();
        assert_eq!(found.len(), 2);
        assert!(find_by_names(&table, &["missing".into()]).is_err());
        assert!(is_app(&to_entries(&table)[2]));
    }
}
