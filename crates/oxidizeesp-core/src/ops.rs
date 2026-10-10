//! Hardware operations.
//!
//! Each function performs one user-level operation on a leased device:
//! validate inputs, connect, verify the device identity, run the operation
//! with progress reporting, and apply the configured post-operation reset.

use std::{borrow::Cow, path::Path, time::Instant};

use espflash::{image_format::Segment, target::ProgressCallbacks};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    Error, ErrorKind, Result,
    backup::{self, BackupMetadata},
    error::display_name,
    fsutil::{self, AtomicFile},
    hardware::{Backend, ChipInfo, FlashLink, WriteBehavior},
    image::{self, FileKind, ImageOptions, ImageSummary, OwnedSegment},
    job::{JobContext, ProgressUnit, Stage},
    model::{Chip, ConnectionOptions, FlashSize, ROM_BAUD, ResetAfter},
    partitions::{self, DEFAULT_TABLE_OFFSET, MAX_TABLE_SIZE, PartitionEntry, PartitionTableView},
    region::{Alignment, FlashRegion, SECTOR_SIZE, format_address, format_size},
    session::{Lease, now_rfc3339, verify_identity},
};

/// Read size per espflash `read_flash` call. Bounded so that progress and
/// cancellation stay responsive and memory use stays small.
const READ_CHUNK: u32 = 0x4_0000;
/// Erase size per command for region erases.
const ERASE_CHUNK: u32 = 0x1_0000;

pub const APP_NAME: &str = "OxidizeESP";

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FlashElfRequest {
    pub device_id: String,
    pub elf_path: String,
    pub connection: ConnectionOptions,
    pub image: ImageOptions,
    pub verify: bool,
    pub skip_unchanged: bool,
    /// Partitions (by label) to erase before writing the image.
    pub erase_partitions: Vec<String>,
    pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WriteBinRequest {
    pub device_id: String,
    pub path: String,
    pub offset: u32,
    pub connection: ConnectionOptions,
    pub verify: bool,
    pub skip_unchanged: bool,
    pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FlashResult {
    pub chip_info: ChipInfo,
    /// Present for ELF flashing.
    pub image: Option<ImageSummary>,
    pub regions: Vec<FlashRegion>,
    #[ts(type = "number")]
    pub bytes_written: u64,
    pub verified: bool,
    pub skipped_regions: Vec<FlashRegion>,
    #[ts(type = "number")]
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReadFlashRequest {
    pub device_id: String,
    /// Region to read; `None` reads the complete flash.
    pub region: Option<FlashRegion>,
    pub output_path: String,
    pub connection: ConnectionOptions,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BackupResult {
    pub path: String,
    pub metadata_path: String,
    pub metadata: BackupMetadata,
    #[ts(type = "number")]
    pub duration_ms: u64,
}

/// Where to obtain the partition table for erasing named partitions.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum PartitionSource {
    Device { offset: u32 },
    File { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum EraseTarget {
    /// Erase the entire flash chip.
    Full,
    Region { region: FlashRegion },
    Partitions { names: Vec<String>, source: PartitionSource },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EraseRequest {
    pub device_id: String,
    pub target: EraseTarget,
    pub connection: ConnectionOptions,
    pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EraseResult {
    /// Erased regions; `None` for a full-chip erase.
    pub regions: Option<Vec<FlashRegion>>,
    pub partitions: Vec<PartitionEntry>,
    #[ts(type = "number")]
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChecksumRequest {
    pub device_id: String,
    pub region: FlashRegion,
    pub connection: ConnectionOptions,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChecksumResult {
    pub region: FlashRegion,
    /// Device-side MD5 digest, lowercase hex.
    pub md5: String,
    #[ts(type = "number")]
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReadPartitionTableRequest {
    pub device_id: String,
    pub offset: u32,
    pub connection: ConnectionOptions,
}

fn require_confirmation(confirmed: bool) -> Result<()> {
    if confirmed {
        Ok(())
    } else {
        Err(Error::invalid_input("This operation modifies flash and must be confirmed."))
    }
}

fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Connect to the leased device and verify that it is the device identified
/// earlier.
fn connect(
    ctx: &mut JobContext<'_>,
    backend: &dyn Backend,
    lease: &Lease,
    options: &ConnectionOptions,
    behavior: WriteBehavior,
) -> Result<(Box<dyn FlashLink>, ChipInfo)> {
    options.validate()?;
    ctx.check_cancelled()?;
    let port = lease.port();
    ctx.stage(Stage::Connecting, format!("Connecting to the bootloader on {}", port.name));
    let expected = lease.known_info().map(|i| i.chip);
    let mut link = backend.connect(port, options, behavior, expected)?;
    let info = link.chip_info()?;
    verify_identity(lease.known_info(), &info)?;
    lease.record_info(info.clone());
    ctx.info(format!(
        "Connected to {}{}{}",
        info.chip_name,
        info.revision.as_ref().map(|r| format!(" (revision v{r})")).unwrap_or_default(),
        info.mac_address.as_ref().map(|m| format!(", MAC {m}")).unwrap_or_default()
    ));
    if info.secure_download_mode() {
        ctx.warn("Secure Download Mode is enabled: verification, flash detection and reads are restricted.");
    }
    if options.baud > ROM_BAUD {
        ctx.info(format!("Using {} baud", options.baud));
    }
    Ok((link, info))
}

fn finish_with_reset(ctx: &mut JobContext<'_>, link: &mut dyn FlashLink, options: &ConnectionOptions) -> Result<()> {
    if options.reset_after != ResetAfter::NoResetNoStub {
        ctx.stage(Stage::Resetting, "Applying post-operation reset");
    }
    link.reset_after()
}

/// Identify the chip on the leased device and restart it.
pub fn probe(ctx: &mut JobContext<'_>, backend: &dyn Backend, lease: &Lease, options: &ConnectionOptions) -> Result<ChipInfo> {
    let options = ConnectionOptions { reset_after: ResetAfter::HardReset, ..options.clone() };
    // Probing must also work after the user swapped the board, so the
    // previously known identity is not enforced here.
    options.validate()?;
    ctx.stage(Stage::Connecting, format!("Connecting to the bootloader on {}", lease.port().name));
    let mut link = backend.connect(lease.port(), &options, WriteBehavior::default(), None)?;
    let info = link.chip_info()?;
    lease.record_info(info.clone());
    ctx.stage(Stage::Resetting, "Restarting the application");
    link.reset_after()?;
    Ok(info)
}

/// Enter the ROM bootloader and leave the chip there, so the application
/// does not run (equivalent to `espflash hold-in-reset`).
pub fn hold_in_reset(ctx: &mut JobContext<'_>, backend: &dyn Backend, lease: &Lease, options: &ConnectionOptions) -> Result<ChipInfo> {
    let options = ConnectionOptions { reset_after: ResetAfter::NoReset, ..options.clone() };
    let (_link, info) = connect(ctx, backend, lease, &options, WriteBehavior::default())?;
    Ok(info)
}

/// Restart the chip through the DTR/RTS lines without entering the
/// bootloader.
pub fn reset(ctx: &mut JobContext<'_>, backend: &dyn Backend, lease: &Lease) -> Result<()> {
    ctx.stage(Stage::Resetting, format!("Resetting the device on {}", lease.port().name));
    let mut serial = backend.open_serial(lease.port(), ROM_BAUD)?;
    serial.reset()
}

/// Progress adapter from espflash's per-segment callbacks to job events.
///
/// espflash reports the number of write blocks for each segment in `init`
/// and the completed block count in `update`. Overall progress is weighted by
/// segment size.
struct WriteProgress<'c, 'a> {
    ctx: &'c mut JobContext<'a>,
    segments: Vec<(String, FlashRegion)>,
    index: usize,
    blocks_total: usize,
    done_bytes: u64,
    total_bytes: u64,
    skipped: Vec<FlashRegion>,
}

impl<'c, 'a> WriteProgress<'c, 'a> {
    fn new(ctx: &'c mut JobContext<'a>, segments: Vec<(String, FlashRegion)>) -> Self {
        let total_bytes = segments.iter().map(|(_, r)| u64::from(r.length)).sum::<u64>().max(1);
        Self { ctx, segments, index: 0, blocks_total: 1, done_bytes: 0, total_bytes, skipped: Vec::new() }
    }

    fn current(&self) -> Option<(String, FlashRegion)> {
        self.segments.get(self.index).cloned()
    }
}

impl ProgressCallbacks for WriteProgress<'_, '_> {
    fn init(&mut self, addr: u32, total: usize) {
        if let Some(pos) = self.segments.iter().position(|(_, r)| r.offset == addr) {
            self.index = pos;
        }
        self.blocks_total = total.max(1);
        let label = self.current().map(|(l, r)| format!("{l} at {}", format_address(r.offset)));
        self.ctx.mark_flash_touched();
        self.ctx.stage(Stage::Writing, format!("Writing {}", label.unwrap_or_else(|| format_address(addr))));
    }

    fn update(&mut self, current: usize) {
        let Some((_, region)) = self.current() else { return };
        let fraction = current as f64 / self.blocks_total as f64;
        let segment_done = (f64::from(region.length) * fraction) as u64;
        let overall = (self.done_bytes + segment_done) as f64 / self.total_bytes as f64;
        self.ctx.progress(
            current as u64,
            self.blocks_total as u64,
            ProgressUnit::Blocks,
            overall,
            Some(format_address(region.offset)),
        );
    }

    fn verifying(&mut self) {
        if let Some((label, region)) = self.current() {
            self.ctx.stage(Stage::Verifying, format!("Verifying {label} at {}", format_address(region.offset)));
        }
    }

    fn finish(&mut self, skipped: bool) {
        if let Some((label, region)) = self.current() {
            if skipped {
                self.skipped.push(region);
                self.ctx.info(format!("{label} at {} is unchanged and was skipped", format_address(region.offset)));
            }
            self.done_bytes += u64::from(region.length);
        }
        self.index += 1;
        let overall = self.done_bytes as f64 / self.total_bytes as f64;
        self.ctx.progress(self.done_bytes, self.total_bytes, ProgressUnit::Bytes, overall, None);
    }
}

fn write_segments(
    ctx: &mut JobContext<'_>,
    link: &mut dyn FlashLink,
    segments: &[OwnedSegment],
    labels: &[String],
) -> Result<Vec<FlashRegion>> {
    let regions: Vec<(String, FlashRegion)> = segments
        .iter()
        .zip(labels.iter().cloned().chain(std::iter::repeat("Segment".to_owned())))
        .map(|(s, label)| (label, FlashRegion::new(s.addr, s.data.len() as u32)))
        .collect();
    let ef_segments: Vec<Segment<'_>> = segments
        .iter()
        .map(|s| Segment { addr: s.addr, data: Cow::Borrowed(&s.data) })
        .collect();
    ctx.check_cancelled()?;
    let mut progress = WriteProgress::new(ctx, regions);
    link.write_segments(&ef_segments, &mut progress)?;
    Ok(progress.skipped)
}

fn effective_flash_size(info: &ChipInfo, override_size: Option<FlashSize>, ctx: &JobContext<'_>) -> FlashSize {
    if let Some(size) = override_size {
        return size;
    }
    match info.flash_size {
        Some(size) => size,
        None => {
            ctx.warn("The flash size could not be detected; assuming 4 MiB.");
            FlashSize::Mb4
        }
    }
}

/// Flash an application ELF using the ESP-IDF image format.
pub fn flash_elf(ctx: &mut JobContext<'_>, backend: &dyn Backend, lease: &Lease, request: &FlashElfRequest) -> Result<FlashResult> {
    require_confirmation(request.confirmed)?;
    let start = Instant::now();
    ctx.stage(Stage::Preparing, "Reading firmware");
    let elf_path = Path::new(&request.elf_path);
    let elf = image::read_elf(elf_path)?;
    let elf_info = image::inspect_elf_data(elf_path, &elf)?;
    if let Some(known) = lease.known_info() {
        image::check_chip_compatibility(&elf_info, known.chip)?;
    }

    let behavior = WriteBehavior { verify: request.verify, skip_unchanged: request.skip_unchanged };
    let (mut link, info) = connect(ctx, backend, lease, &request.connection, behavior)?;
    image::check_chip_compatibility(&elf_info, info.chip)?;

    let sdm = info.secure_download_mode();
    let min_rev = request.image.min_chip_revision.unwrap_or(0).max(elf_info.min_chip_revision.unwrap_or(0));
    if !sdm
        && min_rev > 0
        && let Some(revision) = info.revision_number
        && revision < min_rev
    {
        return Err(Error::new(
            ErrorKind::IncompatibleFirmware,
            format!(
                "The firmware requires chip revision v{}.{} or newer, but the device is v{}.",
                min_rev / 100,
                min_rev % 100,
                info.revision.clone().unwrap_or_default()
            ),
        ));
    }

    let flash_size = effective_flash_size(&info, request.image.flash.size, ctx);
    if request.image.flash.size.is_some() {
        link.set_flash_size(flash_size);
    }

    ctx.stage(Stage::Preparing, "Building application image");
    let prepared = image::prepare(&elf, info.chip, info.crystal, &request.image, flash_size, info.revision_number)?;
    for warning in &prepared.summary.warnings {
        ctx.warn(warning.clone());
    }
    ctx.info(format!(
        "Application size {} of {} partition",
        format_size(u64::from(prepared.summary.app_size)),
        format_size(u64::from(prepared.summary.app_partition_size))
    ));

    if !request.erase_partitions.is_empty() {
        let entries = prepared
            .summary
            .partition_table
            .entries
            .iter()
            .filter(|e| request.erase_partitions.contains(&e.name))
            .cloned()
            .collect::<Vec<_>>();
        if entries.len() != request.erase_partitions.len() {
            return Err(Error::invalid_input("One or more partitions selected for erasing do not exist in the partition table."));
        }
        erase_entries(ctx, link.as_mut(), &entries)?;
    }

    let labels: Vec<String> = prepared.summary.segments.iter().map(|s| s.label.clone()).collect();
    let skipped = write_segments(ctx, link.as_mut(), &prepared.segments, &labels)?;
    let regions = prepared
        .segments
        .iter()
        .map(|s| FlashRegion::new(s.addr, s.data.len() as u32))
        .collect::<Vec<_>>();
    let bytes_written = regions
        .iter()
        .filter(|r| !skipped.contains(r))
        .map(|r| u64::from(r.length))
        .sum();

    Ok(FlashResult {
        chip_info: info,
        image: Some(prepared.summary),
        regions,
        bytes_written,
        verified: request.verify && !sdm,
        skipped_regions: skipped,
        duration_ms: elapsed_ms(start),
    })
}

/// Validate a raw binary against the connected chip where its format allows.
fn check_binary(ctx: &JobContext<'_>, data: &[u8], offset: u32, chip: Chip, path: &Path) -> Result<()> {
    match image::detect_kind(data) {
        FileKind::Elf => {
            return Err(Error::invalid_file(
                "The selected file is an ELF executable. Use \"Flash application\" to flash ELF files.",
            ));
        }
        FileKind::EspImage => {
            // ESP image header: the chip ID is stored little-endian at byte 12
            // of the extended header.
            if data.len() >= 24 {
                let chip_id = u16::from_le_bytes([data[12], data[13]]);
                if chip_id != chip.to_espflash().id() {
                    let image_chip = Chip::ALL
                        .into_iter()
                        .find(|c| c.to_espflash().id() == chip_id)
                        .map(|c| c.display_name().to_owned())
                        .unwrap_or_else(|| format!("chip ID {chip_id}"));
                    return Err(Error::new(
                        ErrorKind::IncompatibleFirmware,
                        format!(
                            "The image header targets {image_chip}, but the connected chip is {}.",
                            chip.display_name()
                        ),
                    ));
                }
            }
        }
        FileKind::PartitionTable => {
            if offset != DEFAULT_TABLE_OFFSET {
                ctx.warn(format!(
                    "The file is a partition table but is written to {} instead of the default {}.",
                    format_address(offset),
                    format_address(DEFAULT_TABLE_OFFSET)
                ));
            }
        }
        FileKind::Raw => {}
    }
    if let Ok(Some(meta)) = backup::read_sidecar(path) {
        if meta.chip != chip {
            return Err(Error::new(
                ErrorKind::IncompatibleFirmware,
                format!(
                    "This backup was taken from an {}, but the connected chip is {}.",
                    meta.chip.display_name(),
                    chip.display_name()
                ),
            ));
        }
        if meta.flash_offset != offset {
            ctx.warn(format!(
                "This backup was read from {}, but it is being written to {}.",
                format_address(meta.flash_offset),
                format_address(offset)
            ));
        }
        if meta.flash_encryption == Some(true) {
            ctx.warn("This backup was taken from a device with flash encryption enabled. Its contents are only valid on the original device.");
        }
    }
    Ok(())
}

/// Write a raw binary to a specific flash offset.
pub fn write_bin(ctx: &mut JobContext<'_>, backend: &dyn Backend, lease: &Lease, request: &WriteBinRequest) -> Result<FlashResult> {
    require_confirmation(request.confirmed)?;
    let start = Instant::now();
    let path = Path::new(&request.path);
    fsutil::require_absolute(path)?;
    ctx.stage(Stage::Preparing, format!("Reading {}", display_name(path)));
    let mut data = fsutil::read_input(path, fsutil::MAX_INPUT_FILE_SIZE)?;
    let length = u32::try_from(data.len()).map_err(|_| Error::invalid_file("The file is too large."))?;
    let region = FlashRegion::new(request.offset, length);
    // Flash is erased in 4 KiB sectors starting at the write offset, so an
    // unaligned offset would erase data before it.
    if !request.offset.is_multiple_of(SECTOR_SIZE) {
        return Err(Error::invalid_input(format!(
            "The offset {} must be aligned to the 4 KiB sector size (0x1000).",
            format_address(request.offset)
        )));
    }
    region.validate(Alignment::None, None)?;
    if let Some(known) = lease.known_info() {
        check_binary(ctx, &data, request.offset, known.chip, path)?;
        if let Some(size) = known.flash_size_bytes() {
            region.validate(Alignment::None, Some(size))?;
        }
    }

    let behavior = WriteBehavior { verify: request.verify, skip_unchanged: request.skip_unchanged };
    let (mut link, info) = connect(ctx, backend, lease, &request.connection, behavior)?;
    check_binary(ctx, &data, request.offset, info.chip, path)?;
    if let Some(size) = info.flash_size_bytes() {
        region.validate(Alignment::None, Some(size))?;
    }

    // espflash pads to a 4-byte boundary with erased-flash bytes.
    while data.len() % 4 != 0 {
        data.push(0xFF);
    }
    let segment = OwnedSegment { addr: request.offset, data };
    let skipped = write_segments(ctx, link.as_mut(), std::slice::from_ref(&segment), &[display_name(path)])?;
    let sdm = info.secure_download_mode();
    Ok(FlashResult {
        chip_info: info,
        image: None,
        regions: vec![region],
        bytes_written: if skipped.is_empty() { u64::from(length) } else { 0 },
        verified: request.verify && !sdm,
        skipped_regions: skipped,
        duration_ms: elapsed_ms(start),
    })
}

/// Read a flash region into a file, finalizing it only after the complete
/// region was read successfully.
pub fn read_flash(ctx: &mut JobContext<'_>, backend: &dyn Backend, lease: &Lease, request: &ReadFlashRequest) -> Result<BackupResult> {
    let start = Instant::now();
    let output = Path::new(&request.output_path);
    fsutil::validate_output(output)?;
    if let Some(region) = &request.region {
        region.validate(Alignment::None, lease.known_info().and_then(ChipInfo::flash_size_bytes))?;
    }

    let (mut link, info) = connect(ctx, backend, lease, &request.connection, WriteBehavior::default())?;
    if info.secure_download_mode() {
        return Err(Error::unsupported("Reading flash is not possible while Secure Download Mode is enabled."));
    }
    let flash_size = info.flash_size_bytes();
    let (region, full_flash) = match request.region {
        Some(region) => {
            region.validate(Alignment::None, flash_size)?;
            (region, region.offset == 0 && Some(region.length) == flash_size)
        }
        None => {
            let size = flash_size.ok_or_else(|| {
                Error::unsupported("The flash size could not be detected. Specify the region to read explicitly.")
            })?;
            (FlashRegion::new(0, size), true)
        }
    };
    if info.security.as_ref().is_some_and(|s| s.flash_encryption) {
        ctx.warn("Flash encryption is enabled. The backup contains encrypted data that is only meaningful on this device.");
    }

    let mut file = AtomicFile::create(output)?;
    let scratch = tempfile::Builder::new()
        .prefix("oxidizeesp-read-")
        .tempdir()
        .map_err(|e| Error::new(ErrorKind::Io, "A temporary folder could not be created.").with_detail(e.to_string()))?;
    let chunk_path = scratch.path().join("chunk.bin");

    ctx.stage(
        Stage::Reading,
        format!("Reading {} from {}", format_size(u64::from(region.length)), format_address(region.offset)),
    );
    let total = u64::from(region.length);
    ctx.progress(0, total, ProgressUnit::Bytes, 0.0, Some(format_address(region.offset)));
    for chunk in region.chunks(READ_CHUNK) {
        ctx.check_cancelled()?;
        link.read_flash(chunk.offset, chunk.length, &chunk_path)?;
        let data = std::fs::read(&chunk_path).map_err(|e| Error::file_io(&e, &chunk_path, "read"))?;
        if data.len() != chunk.length as usize {
            return Err(Error::new(
                ErrorKind::Connection,
                format!("The device returned {} bytes instead of {}.", data.len(), chunk.length),
            ));
        }
        file.write(&data)?;
        let done = file.written();
        ctx.progress(done, total, ProgressUnit::Bytes, done as f64 / total as f64, Some(format_address(chunk.offset)));
    }

    ctx.stage(Stage::Finalizing, "Saving backup");
    let sha256 = file.commit()?;
    let metadata = BackupMetadata {
        format_version: backup::METADATA_FORMAT_VERSION,
        application: APP_NAME.to_owned(),
        application_version: env!("CARGO_PKG_VERSION").to_owned(),
        created_at: now_rfc3339(),
        chip: info.chip,
        chip_revision: info.revision.clone(),
        mac_address: info.mac_address.clone(),
        usb_serial_number: lease.port().usb.as_ref().and_then(|u| u.serial_number.clone()),
        port_name: lease.port().name.clone(),
        flash_offset: region.offset,
        length: region.length,
        flash_size,
        sha256,
        flash_encryption: info.security.as_ref().map(|s| s.flash_encryption),
        full_flash,
    };
    if let Err(err) = backup::write_sidecar(output, &metadata) {
        ctx.warn(format!("The backup was saved, but its metadata file could not be written: {}", err.message));
    }
    finish_with_reset(ctx, link.as_mut(), &request.connection)?;

    Ok(BackupResult {
        path: request.output_path.clone(),
        metadata_path: backup::sidecar_path(output).display().to_string(),
        metadata,
        duration_ms: elapsed_ms(start),
    })
}

fn read_table(
    ctx: &mut JobContext<'_>,
    link: &mut dyn FlashLink,
    offset: u32,
    flash_size: Option<u32>,
) -> Result<PartitionTableView> {
    if !offset.is_multiple_of(SECTOR_SIZE) {
        return Err(Error::invalid_input("The partition table offset must be 4 KiB aligned."));
    }
    ctx.stage(Stage::Reading, format!("Reading partition table at {}", format_address(offset)));
    let scratch = tempfile::NamedTempFile::new()
        .map_err(|e| Error::new(ErrorKind::Io, "A temporary file could not be created.").with_detail(e.to_string()))?;
    link.read_flash(offset, MAX_TABLE_SIZE, scratch.path())?;
    let data = std::fs::read(scratch.path()).map_err(|e| Error::file_io(&e, scratch.path(), "read"))?;
    if data.iter().take(32).all(|b| *b == 0xFF) {
        return Err(Error::invalid_file(format!(
            "No partition table was found at {}: the flash is erased there.",
            format_address(offset)
        )));
    }
    if !data.starts_with(&[0xAA, 0x50]) {
        return Err(Error::invalid_file(format!(
            "No partition table was found at {}.",
            format_address(offset)
        )));
    }
    let (table, format) = partitions::parse(&data)?;
    Ok(partitions::view(&table, format, flash_size, offset))
}

/// Read and parse the partition table stored on the device.
pub fn read_partition_table(
    ctx: &mut JobContext<'_>,
    backend: &dyn Backend,
    lease: &Lease,
    request: &ReadPartitionTableRequest,
) -> Result<PartitionTableView> {
    let (mut link, info) = connect(ctx, backend, lease, &request.connection, WriteBehavior::default())?;
    if info.secure_download_mode() {
        return Err(Error::unsupported("Reading flash is not possible while Secure Download Mode is enabled."));
    }
    let view = read_table(ctx, link.as_mut(), request.offset, info.flash_size_bytes())?;
    finish_with_reset(ctx, link.as_mut(), &request.connection)?;
    Ok(view)
}

fn erase_regions(ctx: &mut JobContext<'_>, link: &mut dyn FlashLink, regions: &[FlashRegion]) -> Result<()> {
    let total: u64 = regions.iter().map(|r| u64::from(r.length)).sum::<u64>().max(1);
    let mut done = 0u64;
    for region in regions {
        ctx.stage(
            Stage::Erasing,
            format!("Erasing {} at {}", format_size(u64::from(region.length)), format_address(region.offset)),
        );
        for chunk in region.chunks(ERASE_CHUNK) {
            ctx.check_cancelled()?;
            ctx.mark_flash_touched();
            link.erase_region(chunk.offset, chunk.length)?;
            done += u64::from(chunk.length);
            ctx.progress(done, total, ProgressUnit::Bytes, done as f64 / total as f64, Some(format_address(chunk.offset)));
        }
    }
    Ok(())
}

fn erase_entries(ctx: &mut JobContext<'_>, link: &mut dyn FlashLink, entries: &[PartitionEntry]) -> Result<()> {
    let regions: Vec<FlashRegion> = entries.iter().map(|e| FlashRegion::new(e.offset, e.size)).collect();
    for (entry, region) in entries.iter().zip(&regions) {
        region.validate(Alignment::Sector, None).map_err(|_| {
            Error::invalid_input(format!("Partition \"{}\" is not sector aligned and cannot be erased.", entry.name))
        })?;
    }
    erase_regions(ctx, link, &regions)
}

/// Erase the whole chip, a region, or named partitions.
pub fn erase(ctx: &mut JobContext<'_>, backend: &dyn Backend, lease: &Lease, request: &EraseRequest) -> Result<EraseResult> {
    require_confirmation(request.confirmed)?;
    if !request.connection.use_stub {
        return Err(Error::unsupported("Erasing requires the RAM flasher stub.")
            .with_hint("Enable \"Use flasher stub\" in the connection options."));
    }
    let start = Instant::now();
    let known_size = lease.known_info().and_then(ChipInfo::flash_size_bytes);

    // Validate everything that does not need the device before connecting.
    let file_table = match &request.target {
        EraseTarget::Region { region } => {
            region.validate(Alignment::Sector, known_size)?;
            None
        }
        EraseTarget::Partitions { names, source } => {
            if names.is_empty() {
                return Err(Error::invalid_input("Select at least one partition to erase."));
            }
            match source {
                PartitionSource::File { path } => {
                    let path = Path::new(path);
                    fsutil::require_absolute(path)?;
                    let (table, _) = partitions::load(path)?;
                    Some(partitions::find_by_names(&table, names)?)
                }
                PartitionSource::Device { offset } => {
                    if !offset.is_multiple_of(SECTOR_SIZE) {
                        return Err(Error::invalid_input("The partition table offset must be 4 KiB aligned."));
                    }
                    None
                }
            }
        }
        EraseTarget::Full => None,
    };

    let (mut link, info) = connect(ctx, backend, lease, &request.connection, WriteBehavior::default())?;
    let flash_size = info.flash_size_bytes();

    let result = match &request.target {
        EraseTarget::Full => {
            ctx.stage(Stage::Erasing, "Erasing the entire flash. This can take a minute or more.");
            ctx.mark_flash_touched();
            link.erase_all()?;
            EraseResult { regions: None, partitions: vec![], duration_ms: 0 }
        }
        EraseTarget::Region { region } => {
            region.validate(Alignment::Sector, flash_size)?;
            erase_regions(ctx, link.as_mut(), std::slice::from_ref(region))?;
            EraseResult { regions: Some(vec![*region]), partitions: vec![], duration_ms: 0 }
        }
        EraseTarget::Partitions { names, source } => {
            let entries = match (file_table, source) {
                (Some(entries), _) => entries,
                (None, PartitionSource::File { .. }) => {
                    return Err(Error::internal("The partition table file was not loaded."));
                }
                (None, PartitionSource::Device { offset }) => {
                    let view = read_table(ctx, link.as_mut(), *offset, flash_size)?;
                    let mut selected = Vec::new();
                    for name in names {
                        let entry = view
                            .entries
                            .iter()
                            .find(|e| &e.name == name)
                            .ok_or_else(|| Error::invalid_input(format!("The partition \"{name}\" does not exist on the device.")))?;
                        selected.push(entry.clone());
                    }
                    selected
                }
            };
            for entry in &entries {
                FlashRegion::new(entry.offset, entry.size).validate(Alignment::Sector, flash_size)?;
            }
            erase_entries(ctx, link.as_mut(), &entries)?;
            let regions = entries.iter().map(|e| FlashRegion::new(e.offset, e.size)).collect();
            EraseResult { regions: Some(regions), partitions: entries, duration_ms: 0 }
        }
    };
    finish_with_reset(ctx, link.as_mut(), &request.connection)?;
    Ok(EraseResult { duration_ms: elapsed_ms(start), ..result })
}

/// Calculate the device-side MD5 checksum of a flash region.
pub fn checksum(ctx: &mut JobContext<'_>, backend: &dyn Backend, lease: &Lease, request: &ChecksumRequest) -> Result<ChecksumResult> {
    let start = Instant::now();
    request
        .region
        .validate(Alignment::None, lease.known_info().and_then(ChipInfo::flash_size_bytes))?;
    let (mut link, info) = connect(ctx, backend, lease, &request.connection, WriteBehavior::default())?;
    request.region.validate(Alignment::None, info.flash_size_bytes())?;
    ctx.stage(
        Stage::Reading,
        format!(
            "Calculating MD5 of {} at {}",
            format_size(u64::from(request.region.length)),
            format_address(request.region.offset)
        ),
    );
    let digest = link.checksum_md5(request.region.offset, request.region.length)?;
    finish_with_reset(ctx, link.as_mut(), &request.connection)?;
    Ok(ChecksumResult {
        region: request.region,
        md5: format!("{digest:032x}"),
        duration_ms: elapsed_ms(start),
    })
}
