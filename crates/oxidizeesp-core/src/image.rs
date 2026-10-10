//! ELF inspection and ESP-IDF application image generation.
//!
//! Image generation is independent of any connected device: the caller
//! supplies the target chip and crystal frequency explicitly.

use std::path::{Path, PathBuf};

use espflash::{
    flasher::{FlashData, FlashSettings as EfFlashSettings},
    image_format::{ImageFormat, Metadata, idf::IdfBootloaderFormat},
};
use object::{Object, ObjectSymbol};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    Error, ErrorKind, Result,
    error::display_name,
    fsutil,
    model::{Chip, FlashFrequency, FlashMode, FlashSettings, FlashSize, XtalFreq},
    partitions::{self, PartitionTableView, TableFormat},
    region::{FlashRegion, format_address},
};

/// Kind of a firmware file, detected from its content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum FileKind {
    /// 32-bit ELF executable.
    Elf,
    /// ESP application or bootloader image (magic byte 0xE9).
    EspImage,
    /// Binary partition table.
    PartitionTable,
    /// Anything else, written verbatim.
    Raw,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileInfo {
    pub path: String,
    pub file_name: String,
    #[ts(type = "number")]
    pub size: u64,
    pub kind: FileKind,
    pub sha256: String,
    /// Present when the file is a backup created by OxidizeESP.
    pub backup: Option<crate::backup::BackupMetadata>,
}

/// Facts extracted from an ELF file without a device.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ElfInfo {
    pub file: FileInfo,
    /// Chip recorded in the ELF metadata by esp-hal or ESP-IDF builds.
    pub chip: Option<Chip>,
    /// Raw chip name from the metadata, also when it is not supported.
    pub chip_name: Option<String>,
    pub has_app_descriptor: bool,
    /// Whether the ELF was built with esp-hal (contains `.espressif.metadata`).
    pub is_esp_hal: bool,
    /// Minimum chip revision declared by the image (`major * 100 + minor`).
    pub min_chip_revision: Option<u16>,
    pub entry_point: u32,
    /// Log format declared by the image (`defmt` or `serial`).
    pub log_format: Option<String>,
}

/// Options for building an ESP-IDF application image from an ELF.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImageOptions {
    pub flash: FlashSettings,
    /// Custom second-stage bootloader binary.
    pub bootloader: Option<String>,
    /// Custom partition table (CSV or binary).
    pub partition_table: Option<String>,
    pub partition_table_offset: Option<u32>,
    /// Label of the app partition to place the application in.
    pub target_app_partition: Option<String>,
    /// Minimum chip revision (`major * 100 + minor`).
    pub min_chip_revision: Option<u16>,
    pub mmu_page_size: Option<u32>,
    /// Require an ESP-IDF application descriptor, as espflash does by default.
    pub check_app_descriptor: bool,
}

/// A flash segment of a prepared image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedSegment {
    pub addr: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SegmentInfo {
    pub label: String,
    pub offset: u32,
    pub size: u32,
}

/// Image ready to be flashed or saved.
#[derive(Debug, Clone)]
pub struct PreparedImage {
    pub segments: Vec<OwnedSegment>,
    pub summary: ImageSummary,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImageSummary {
    pub chip: Chip,
    pub segments: Vec<SegmentInfo>,
    pub app_size: u32,
    /// Size of the app partition the image is placed in.
    pub app_partition_size: u32,
    pub flash_size: FlashSize,
    pub flash_mode: Option<FlashMode>,
    pub flash_frequency: FlashFrequency,
    pub partition_table: PartitionTableView,
    pub warnings: Vec<String>,
}

/// Detect the kind of a file from its leading bytes.
pub fn detect_kind(data: &[u8]) -> FileKind {
    if data.starts_with(b"\x7FELF") {
        FileKind::Elf
    } else if data.starts_with(&[0xAA, 0x50]) {
        FileKind::PartitionTable
    } else if data.first() == Some(&0xE9) {
        FileKind::EspImage
    } else {
        FileKind::Raw
    }
}

pub fn file_info(path: &Path, data: &[u8]) -> FileInfo {
    FileInfo {
        path: path.display().to_string(),
        file_name: display_name(path),
        size: data.len() as u64,
        kind: detect_kind(data),
        sha256: fsutil::sha256_hex(data),
        backup: crate::backup::read_sidecar(path).ok().flatten(),
    }
}

/// Inspect an arbitrary input file.
pub fn inspect_file(path: &Path) -> Result<FileInfo> {
    fsutil::require_absolute(path)?;
    let data = fsutil::read_input(path, fsutil::MAX_INPUT_FILE_SIZE)?;
    Ok(file_info(path, &data))
}

pub fn read_elf(path: &Path) -> Result<Vec<u8>> {
    fsutil::require_absolute(path)?;
    let data = fsutil::read_input(path, fsutil::MAX_INPUT_FILE_SIZE)?;
    if detect_kind(&data) != FileKind::Elf {
        return Err(Error::invalid_file(format!(
            "\"{}\" is not an ELF file. Use \"Write binary\" for raw images.",
            display_name(path)
        )));
    }
    Ok(data)
}

pub fn inspect_elf(path: &Path) -> Result<ElfInfo> {
    let data = read_elf(path)?;
    inspect_elf_data(path, &data)
}

pub fn inspect_elf_data(path: &Path, data: &[u8]) -> Result<ElfInfo> {
    let object = object::File::parse(data)
        .map_err(|e| Error::invalid_file("The ELF file could not be parsed.").with_detail(e.to_string()))?;
    if object.is_64() {
        return Err(Error::invalid_file("The ELF file is 64-bit; Espressif firmware is always 32-bit."));
    }
    let metadata = Metadata::from_bytes(Some(data));
    let chip_name = metadata.chip_name().map(str::to_owned);
    let has_app_descriptor = object.symbols().any(|s| s.name() == Ok("esp_app_desc"))
        || object.section_by_name(".flash.appdesc").is_some()
        || object.section_by_name(".rodata_desc").is_some();
    Ok(ElfInfo {
        file: file_info(path, data),
        chip: chip_name.as_deref().and_then(Chip::parse),
        chip_name,
        has_app_descriptor,
        is_esp_hal: object.section_by_name(".espressif.metadata").is_some(),
        min_chip_revision: metadata.min_chip_revision(),
        entry_point: u32::try_from(object.entry()).unwrap_or(0),
        log_format: metadata.log_format().map(str::to_owned),
    })
}

/// Ensure an ELF built for a specific chip is not flashed to another one.
pub fn check_chip_compatibility(elf: &ElfInfo, chip: Chip) -> Result<()> {
    match (&elf.chip, &elf.chip_name) {
        (Some(elf_chip), _) if *elf_chip == chip => Ok(()),
        (_, None) => Ok(()),
        (_, Some(name)) => Err(Error::new(
            ErrorKind::IncompatibleFirmware,
            format!("The firmware was built for {name}, but the target chip is {}.", chip.display_name()),
        )
        .with_hint("Select the matching firmware or target chip.")),
    }
}

fn validate_options(chip: Chip, options: &ImageOptions) -> Result<()> {
    if let Some(page) = options.mmu_page_size {
        let valid = chip.to_espflash().valid_mmu_page_sizes();
        let ok = match valid {
            Some(sizes) => sizes.contains(&page),
            None => false,
        };
        if !ok {
            return Err(Error::invalid_input(format!(
                "An MMU page size of {page:#x} is not supported by the {}.",
                chip.display_name()
            )));
        }
    }
    if let Some(offset) = options.partition_table_offset
        && (!offset.is_multiple_of(0x1000) || offset < 0x8000)
    {
        return Err(Error::invalid_input("The partition table offset must be 4 KiB aligned and at least 0x8000."));
    }
    if let Some(freq) = options.flash.frequency
        && !chip.to_espflash().flash_frequency_encodings().contains_key(&freq.to_espflash())
    {
        return Err(Error::invalid_input(format!(
            "The flash frequency {} is not supported by the {}.",
            serde_plain_name(&freq),
            chip.display_name()
        )));
    }
    for path in [&options.bootloader, &options.partition_table].into_iter().flatten() {
        let path = Path::new(path);
        fsutil::require_absolute(path)?;
        if !path.is_file() {
            return Err(Error::invalid_file(format!("\"{}\" does not exist.", display_name(path))));
        }
    }
    if let Some(label) = &options.target_app_partition
        && label.trim().is_empty()
    {
        return Err(Error::invalid_input("The target app partition label is empty."));
    }
    Ok(())
}

fn serde_plain_name<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
}

/// Build the flashable segments for an ELF file.
///
/// `flash_size` is the size used when no override is configured (the
/// detected size when flashing, 4 MiB when generating an image offline).
/// `detected_revision` is the silicon revision of a connected chip.
pub fn prepare(
    elf: &[u8],
    chip: Chip,
    xtal: XtalFreq,
    options: &ImageOptions,
    flash_size: FlashSize,
    detected_revision: Option<u16>,
) -> Result<PreparedImage> {
    validate_options(chip, options)?;
    let mut warnings = Vec::new();

    if options.check_app_descriptor {
        espflash::image_format::idf::check_idf_bootloader(&elf.to_vec()).map_err(|e| {
            Error::invalid_file("The ELF file does not contain an ESP-IDF application descriptor.")
                .with_hint("esp-hal applications need the esp_app_desc!() macro from esp-bootloader-esp-idf. Disable the check in the advanced options to flash anyway.")
                .with_detail(e.to_string())
        })?;
    }

    let size = options.flash.size.unwrap_or(flash_size);
    let frequency = options.flash.frequency.unwrap_or_else(|| {
        FlashFrequency::from_espflash(chip.to_espflash().default_flash_frequency()).unwrap_or(FlashFrequency::Mhz40)
    });
    let mut min_chip_rev = options.min_chip_revision.unwrap_or(0);

    // espflash selects the ESP32-P4 bootloader by revision. The public API
    // only considers the image's minimum revision, so raise it to v3.0 when a
    // v3 chip is detected and no custom bootloader is provided.
    if chip == Chip::Esp32p4
        && options.bootloader.is_none()
        && detected_revision.is_some_and(|r| r >= 300)
        && min_chip_rev < 300
    {
        min_chip_rev = 300;
        warnings.push(
            "ESP32-P4 v3 detected: using the v3 bootloader and setting the image's minimum chip revision to v3.0."
                .to_owned(),
        );
    }

    let flash_data = FlashData::new(
        EfFlashSettings::new(
            options.flash.mode.map(FlashMode::to_espflash),
            Some(size.to_espflash()),
            Some(frequency.to_espflash()),
        ),
        min_chip_rev,
        options.mmu_page_size,
        chip.to_espflash(),
        xtal.to_espflash(),
    );

    let partition_path = options.partition_table.as_deref().map(PathBuf::from);
    let bootloader_path = options.bootloader.as_deref().map(PathBuf::from);
    let idf = IdfBootloaderFormat::new(
        elf,
        &flash_data,
        partition_path.as_deref(),
        bootloader_path.as_deref(),
        options.partition_table_offset,
        options.target_app_partition.as_deref(),
    )
    .map_err(|e| crate::error::from_espflash(e, ""))?;

    let table = idf.partition_table();
    let table_offset = options.partition_table_offset.unwrap_or(partitions::DEFAULT_TABLE_OFFSET);
    let format = if options.partition_table.is_some() {
        partition_source_format(options.partition_table.as_deref())
    } else {
        TableFormat::Csv
    };
    let table_view = partitions::view(&table, format, Some(size.bytes()), table_offset);

    let image = ImageFormat::from(idf);
    let metadata = image.metadata();
    let app_size = metadata.get("app_size").and_then(|v| v.parse().ok()).unwrap_or(0);
    let app_partition_size = metadata.get("part_size").and_then(|v| v.parse().ok()).unwrap_or(0);

    let segments: Vec<OwnedSegment> =
        image.flash_segments().into_iter().map(|s| OwnedSegment { addr: s.addr, data: s.data.into_owned() }).collect();

    let labels = ["Bootloader", "Partition table", "Application"];
    let segment_info = segments
        .iter()
        .enumerate()
        .map(|(i, s)| SegmentInfo {
            label: labels.get(i).copied().unwrap_or("Segment").to_owned(),
            offset: s.addr,
            size: s.data.len() as u32,
        })
        .collect::<Vec<_>>();

    for pair in segments.windows(2) {
        let a = FlashRegion::new(pair[0].addr, pair[0].data.len() as u32);
        let b = FlashRegion::new(pair[1].addr, pair[1].data.len() as u32);
        if a.overlaps(&b) {
            return Err(Error::invalid_file(format!(
                "Image segments at {} and {} overlap. Check the partition table offset and bootloader.",
                format_address(a.offset),
                format_address(b.offset)
            )));
        }
    }
    if let Some(last) = segments.iter().filter_map(|s| s.addr.checked_add(s.data.len() as u32)).max()
        && last > size.bytes()
    {
        return Err(Error::new(
            ErrorKind::IncompatibleFirmware,
            format!("The image ends at {}, beyond the configured flash size.", format_address(last)),
        ));
    }
    if app_partition_size > 0 && app_size > app_partition_size {
        return Err(Error::new(
            ErrorKind::IncompatibleFirmware,
            format!("The application ({app_size} bytes) does not fit into its partition ({app_partition_size} bytes)."),
        ));
    }

    Ok(PreparedImage {
        segments,
        summary: ImageSummary {
            chip,
            segments: segment_info,
            app_size,
            app_partition_size,
            flash_size: size,
            flash_mode: options.flash.mode,
            flash_frequency: frequency,
            partition_table: table_view,
            warnings,
        },
    })
}

fn partition_source_format(path: Option<&str>) -> TableFormat {
    path.and_then(|p| std::fs::read(p).ok()).map(|d| partitions::detect_format(&d)).unwrap_or(TableFormat::Csv)
}

/// Request to preview the layout of an image without writing anything.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PreviewImageRequest {
    pub elf_path: String,
    pub chip: Chip,
    pub xtal: Option<XtalFreq>,
    pub options: ImageOptions,
    /// Flash size used when the options do not override it.
    pub flash_size: Option<FlashSize>,
    pub revision: Option<u16>,
}

pub fn preview(request: &PreviewImageRequest) -> Result<ImageSummary> {
    let path = Path::new(&request.elf_path);
    let elf = read_elf(path)?;
    let info = inspect_elf_data(path, &elf)?;
    check_chip_compatibility(&info, request.chip)?;
    let xtal =
        request.xtal.unwrap_or_else(|| XtalFreq::from_espflash(request.chip.to_espflash().default_xtal_frequency()));
    prepare(&elf, request.chip, xtal, &request.options, request.flash_size.unwrap_or(FlashSize::Mb4), request.revision)
        .map(|p| p.summary)
}

/// Request to write an image to disk.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SaveImageRequest {
    pub elf_path: String,
    pub chip: Chip,
    pub xtal: Option<XtalFreq>,
    pub options: ImageOptions,
    /// Combine bootloader, partition table and application into one file
    /// that is flashed at offset 0.
    pub merge: bool,
    /// When merging, do not pad the file to the full flash size.
    pub skip_padding: bool,
    pub output_path: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SaveImageResult {
    pub output_path: String,
    #[ts(type = "number")]
    pub output_size: u64,
    pub sha256: String,
    /// Flash offset where the generated file must be written.
    pub flash_offset: u32,
    pub summary: ImageSummary,
}

/// Generate an image file from an ELF, mirroring `espflash save-image`.
pub fn save_image(request: &SaveImageRequest) -> Result<SaveImageResult> {
    let elf = read_elf(Path::new(&request.elf_path))?;
    let info = inspect_elf_data(Path::new(&request.elf_path), &elf)?;
    check_chip_compatibility(&info, request.chip)?;
    let output = Path::new(&request.output_path);
    fsutil::validate_output(output)?;
    if output == Path::new(&request.elf_path) {
        return Err(Error::invalid_input("The output file must differ from the input ELF."));
    }

    let xtal =
        request.xtal.unwrap_or_else(|| XtalFreq::from_espflash(request.chip.to_espflash().default_xtal_frequency()));
    let prepared = prepare(&elf, request.chip, xtal, &request.options, FlashSize::Mb4, None)?;

    let (bytes, flash_offset) = if request.merge {
        (merge_segments(&prepared.segments, (!request.skip_padding).then_some(prepared.summary.flash_size.bytes()))?, 0)
    } else {
        // Without merging, only the application image is saved, matching
        // espflash. It is written to the start of the app partition.
        let app =
            prepared.segments.last().ok_or_else(|| Error::internal("The image contains no application segment."))?;
        (app.data.clone(), app.addr)
    };

    let sha256 = fsutil::write_atomic(output, &bytes)?;
    Ok(SaveImageResult {
        output_path: request.output_path.clone(),
        output_size: bytes.len() as u64,
        sha256,
        flash_offset,
        summary: prepared.summary,
    })
}

/// Concatenate segments into one image starting at offset 0, filling gaps
/// with erased-flash bytes (0xFF).
pub fn merge_segments(segments: &[OwnedSegment], pad_to: Option<u32>) -> Result<Vec<u8>> {
    let mut sorted: Vec<&OwnedSegment> = segments.iter().collect();
    sorted.sort_by_key(|s| s.addr);
    let mut out: Vec<u8> = Vec::new();
    for segment in sorted {
        let start = segment.addr as usize;
        if start < out.len() {
            return Err(Error::invalid_file("Image segments overlap and cannot be merged."));
        }
        out.resize(start, 0xFF);
        out.extend_from_slice(&segment.data);
    }
    if let Some(size) = pad_to {
        let size = size as usize;
        if out.len() > size {
            return Err(Error::invalid_file("The merged image is larger than the flash size."));
        }
        out.resize(size, 0xFF);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_file_kinds() {
        assert_eq!(detect_kind(b"\x7FELF\x01"), FileKind::Elf);
        assert_eq!(detect_kind(&[0xE9, 0x03]), FileKind::EspImage);
        assert_eq!(detect_kind(&[0xAA, 0x50, 0x01]), FileKind::PartitionTable);
        assert_eq!(detect_kind(&[0x00, 0x01]), FileKind::Raw);
    }

    #[test]
    fn merges_segments_with_padding() {
        let segments =
            vec![OwnedSegment { addr: 0x10, data: vec![2; 4] }, OwnedSegment { addr: 0x0, data: vec![1; 4] }];
        let merged = merge_segments(&segments, Some(0x20)).unwrap();
        assert_eq!(merged.len(), 0x20);
        assert_eq!(&merged[..4], &[1; 4]);
        assert_eq!(&merged[4..0x10], &[0xFF; 12]);
        assert_eq!(&merged[0x10..0x14], &[2; 4]);
        assert_eq!(merged[0x1F], 0xFF);

        let unpadded = merge_segments(&segments, None).unwrap();
        assert_eq!(unpadded.len(), 0x14);
    }

    #[test]
    fn merge_rejects_overlap_and_oversize() {
        let overlapping =
            vec![OwnedSegment { addr: 0x0, data: vec![1; 8] }, OwnedSegment { addr: 0x4, data: vec![2; 4] }];
        assert!(merge_segments(&overlapping, None).is_err());
        let big = vec![OwnedSegment { addr: 0x0, data: vec![1; 8] }];
        assert!(merge_segments(&big, Some(4)).is_err());
    }

    #[test]
    fn rejects_non_elf_input() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fw.bin");
        std::fs::write(&path, [0xE9, 0, 0, 0]).unwrap();
        let err = read_elf(&path).unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidFile);
    }

    #[test]
    fn chip_compatibility() {
        let base = ElfInfo {
            file: FileInfo {
                path: String::new(),
                file_name: String::new(),
                size: 0,
                kind: FileKind::Elf,
                sha256: String::new(),
                backup: None,
            },
            chip: Some(Chip::Esp32c3),
            chip_name: Some("esp32c3".into()),
            has_app_descriptor: true,
            is_esp_hal: true,
            min_chip_revision: None,
            entry_point: 0,
            log_format: None,
        };
        assert!(check_chip_compatibility(&base, Chip::Esp32c3).is_ok());
        assert_eq!(check_chip_compatibility(&base, Chip::Esp32s3).unwrap_err().kind, ErrorKind::IncompatibleFirmware);
        let unknown = ElfInfo { chip: None, chip_name: None, ..base };
        assert!(check_chip_compatibility(&unknown, Chip::Esp32).is_ok());
    }

    #[test]
    fn validates_image_options() {
        let mut options = ImageOptions { mmu_page_size: Some(0x1234), ..Default::default() };
        assert!(validate_options(Chip::Esp32c6, &options).is_err());
        options.mmu_page_size = Some(0x8000);
        assert!(validate_options(Chip::Esp32c6, &options).is_ok());
        assert!(validate_options(Chip::Esp32, &options).is_err());

        let options = ImageOptions { partition_table_offset: Some(0x8800), ..Default::default() };
        assert!(validate_options(Chip::Esp32, &options).is_err());

        let options = ImageOptions {
            flash: FlashSettings { frequency: Some(FlashFrequency::Mhz12), ..Default::default() },
            ..Default::default()
        };
        assert!(validate_options(Chip::Esp32, &options).is_err());
    }
}
