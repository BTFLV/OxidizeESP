//! Tauri command handlers.
//!
//! Handlers only translate between IPC and [`Services`]: hardware work runs
//! on a blocking worker thread, events are forwarded through IPC channels and
//! errors are returned as structured [`Error`] values.

use std::{path::Path, sync::Arc};

use oxidizeesp_core::{
    Error, ErrorKind, Result,
    backup::{self, BackupVerification},
    fsutil,
    hardware::ChipInfo,
    image::{self, ElfInfo, FileInfo, ImageSummary, PreviewImageRequest, SaveImageRequest, SaveImageResult},
    job::{JobEvent, OperationKind},
    model::{Chip, ChipDescriptor, ConnectionOptions},
    monitor::{self, LineEnding, MonitorCommand, MonitorEvent, MonitorRequest},
    ops::{
        self, BackupResult, ChecksumRequest, ChecksumResult, EraseRequest, EraseResult, EraseTarget, FlashElfRequest,
        FlashResult, ReadFlashRequest, ReadPartitionTableRequest, WriteBinRequest,
    },
    partitions::{self, PartitionEntry, PartitionTableView, TableFormat},
    ports::SerialPortDescriptor,
    region::{FlashRegion, format_address},
    service::Services,
    session::{DeviceSnapshot, now_rfc3339},
    settings::{Settings, SettingsStore},
};
use serde::Serialize;
use tauri::{State, ipc::Channel};
use ts_rs::TS;

pub type AppServices = Arc<Services>;

/// Run blocking work on Tauri's blocking thread pool.
async fn blocking<T, F>(f: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| Error::internal("A background task failed.").with_detail(e.to_string()))?
}

fn channel_sink(channel: Channel<JobEvent>) -> impl Fn(JobEvent) + Send + Sync {
    move |event| {
        if let Err(err) = channel.send(event) {
            log::debug!("Job event could not be delivered: {err}");
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub espflash_version: String,
    pub simulated_hardware: bool,
    pub os: String,
    pub arch: String,
    pub settings_warning: Option<String>,
}

pub struct AppMeta {
    pub settings_warning: Option<String>,
}

#[tauri::command]
pub fn app_info(services: State<'_, AppServices>, meta: State<'_, AppMeta>) -> AppInfo {
    AppInfo {
        name: "OxidizeESP".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        espflash_version: crate::ESPFLASH_VERSION.into(),
        simulated_hardware: services.is_simulated(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        settings_warning: meta.settings_warning.clone(),
    }
}

#[tauri::command]
pub fn chip_descriptors() -> Vec<ChipDescriptor> {
    Chip::ALL.into_iter().map(ChipDescriptor::for_chip).collect()
}

#[tauri::command]
pub async fn list_ports(services: State<'_, AppServices>) -> Result<Vec<SerialPortDescriptor>> {
    let services = services.inner().clone();
    blocking(move || services.list_ports()).await
}

#[tauri::command]
pub fn list_devices(services: State<'_, AppServices>) -> Vec<DeviceSnapshot> {
    services.devices().list()
}

#[tauri::command]
pub async fn open_device(services: State<'_, AppServices>, port_name: String) -> Result<DeviceSnapshot> {
    let services = services.inner().clone();
    blocking(move || services.open_device(&port_name)).await
}

#[tauri::command]
pub async fn close_device(services: State<'_, AppServices>, device_id: String) -> Result<()> {
    let services = services.inner().clone();
    blocking(move || services.close_device(&device_id)).await
}

#[tauri::command]
pub async fn probe_device(
    services: State<'_, AppServices>,
    device_id: String,
    connection: ConnectionOptions,
    on_event: Channel<JobEvent>,
) -> Result<ChipInfo> {
    let services = services.inner().clone();
    blocking(move || {
        let sink = channel_sink(on_event);
        services.run_job(
            &device_id,
            OperationKind::Probe,
            &sink,
            |ctx, backend, lease| ops::probe(ctx, backend, lease, &connection),
            |info| format!("Identified {}", info.chip_name),
        )
    })
    .await
}

#[tauri::command]
pub async fn reset_device(
    services: State<'_, AppServices>,
    device_id: String,
    on_event: Channel<JobEvent>,
) -> Result<()> {
    let services = services.inner().clone();
    blocking(move || {
        let sink = channel_sink(on_event);
        services.run_job(
            &device_id,
            OperationKind::Reset,
            &sink,
            |ctx, backend, lease| ops::reset(ctx, backend, lease),
            |()| "Device reset".into(),
        )
    })
    .await
}

#[tauri::command]
pub async fn hold_in_reset(
    services: State<'_, AppServices>,
    device_id: String,
    connection: ConnectionOptions,
    on_event: Channel<JobEvent>,
) -> Result<ChipInfo> {
    let services = services.inner().clone();
    blocking(move || {
        let sink = channel_sink(on_event);
        services.run_job(
            &device_id,
            OperationKind::HoldInReset,
            &sink,
            |ctx, backend, lease| ops::hold_in_reset(ctx, backend, lease, &connection),
            |_| "The chip is held in the bootloader".into(),
        )
    })
    .await
}

#[tauri::command]
pub async fn flash_elf(
    services: State<'_, AppServices>,
    request: FlashElfRequest,
    on_event: Channel<JobEvent>,
) -> Result<FlashResult> {
    let services = services.inner().clone();
    blocking(move || {
        let sink = channel_sink(on_event);
        services.run_job(
            &request.device_id,
            OperationKind::FlashElf,
            &sink,
            |ctx, backend, lease| ops::flash_elf(ctx, backend, lease, &request),
            |r| {
                if r.verified { "Application flashed and verified".into() } else { "Application flashed".into() }
            },
        )
    })
    .await
}

#[tauri::command]
pub async fn write_bin(
    services: State<'_, AppServices>,
    request: WriteBinRequest,
    on_event: Channel<JobEvent>,
) -> Result<FlashResult> {
    let services = services.inner().clone();
    blocking(move || {
        let sink = channel_sink(on_event);
        services.run_job(
            &request.device_id,
            OperationKind::WriteBin,
            &sink,
            |ctx, backend, lease| ops::write_bin(ctx, backend, lease, &request),
            |r| {
                let at = r.regions.first().map(|reg| format_address(reg.offset)).unwrap_or_default();
                if r.verified {
                    format!("Binary written to {at} and verified")
                } else {
                    format!("Binary written to {at}")
                }
            },
        )
    })
    .await
}

#[tauri::command]
pub async fn read_flash(
    services: State<'_, AppServices>,
    request: ReadFlashRequest,
    on_event: Channel<JobEvent>,
) -> Result<BackupResult> {
    let services = services.inner().clone();
    blocking(move || {
        let sink = channel_sink(on_event);
        services.run_job(
            &request.device_id,
            OperationKind::ReadFlash,
            &sink,
            |ctx, backend, lease| ops::read_flash(ctx, backend, lease, &request),
            |_| "Backup saved".into(),
        )
    })
    .await
}

#[tauri::command]
pub async fn read_partition_table(
    services: State<'_, AppServices>,
    request: ReadPartitionTableRequest,
    on_event: Channel<JobEvent>,
) -> Result<PartitionTableView> {
    let services = services.inner().clone();
    blocking(move || {
        let sink = channel_sink(on_event);
        services.run_job(
            &request.device_id,
            OperationKind::ReadPartitionTable,
            &sink,
            |ctx, backend, lease| ops::read_partition_table(ctx, backend, lease, &request),
            |view| format!("Read {} partitions", view.entries.len()),
        )
    })
    .await
}

#[tauri::command]
pub async fn erase(
    services: State<'_, AppServices>,
    request: EraseRequest,
    on_event: Channel<JobEvent>,
) -> Result<EraseResult> {
    let services = services.inner().clone();
    let kind = match request.target {
        EraseTarget::Full => OperationKind::EraseFlash,
        EraseTarget::Region { .. } => OperationKind::EraseRegion,
        EraseTarget::Partitions { .. } => OperationKind::ErasePartitions,
    };
    blocking(move || {
        let sink = channel_sink(on_event);
        services.run_job(
            &request.device_id,
            kind,
            &sink,
            |ctx, backend, lease| ops::erase(ctx, backend, lease, &request),
            |r| match &r.regions {
                None => "The entire flash was erased".into(),
                Some(regions) => format!("Erased {} region(s)", regions.len()),
            },
        )
    })
    .await
}

#[tauri::command]
pub async fn checksum_md5(
    services: State<'_, AppServices>,
    request: ChecksumRequest,
    on_event: Channel<JobEvent>,
) -> Result<ChecksumResult> {
    let services = services.inner().clone();
    blocking(move || {
        let sink = channel_sink(on_event);
        services.run_job(
            &request.device_id,
            OperationKind::ChecksumMd5,
            &sink,
            |ctx, backend, lease| ops::checksum(ctx, backend, lease, &request),
            |r| format!("MD5 {}", r.md5),
        )
    })
    .await
}

#[tauri::command]
pub fn cancel_job(services: State<'_, AppServices>, job_id: String) -> Result<()> {
    services.cancel_job(&job_id)
}

#[tauri::command]
pub async fn inspect_file(path: String) -> Result<FileInfo> {
    blocking(move || image::inspect_file(Path::new(&path))).await
}

#[tauri::command]
pub async fn inspect_elf(path: String) -> Result<ElfInfo> {
    blocking(move || image::inspect_elf(Path::new(&path))).await
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LocalChecksum {
    pub region: FlashRegion,
    /// MD5 of the selected part of the file, lowercase hex.
    pub md5: String,
}

/// MD5 of part of a local file, for comparison with a device-side checksum.
#[tauri::command]
pub async fn file_md5(path: String, offset: u32, length: Option<u32>) -> Result<LocalChecksum> {
    blocking(move || {
        use md5::{Digest, Md5};
        let path = Path::new(&path);
        fsutil::require_absolute(path)?;
        let data = fsutil::read_input(path, fsutil::MAX_INPUT_FILE_SIZE)?;
        let start = offset as usize;
        let len = length.map_or(data.len().saturating_sub(start), |l| l as usize);
        let end = start
            .checked_add(len)
            .filter(|end| *end <= data.len() && len > 0)
            .ok_or_else(|| Error::invalid_input("The selected range lies outside the file."))?;
        let digest = Md5::digest(&data[start..end]);
        Ok(LocalChecksum {
            region: FlashRegion::new(offset, u32::try_from(len).unwrap_or(u32::MAX)),
            md5: fsutil::hex(&digest),
        })
    })
    .await
}

#[tauri::command]
pub async fn verify_backup(path: String) -> Result<BackupVerification> {
    blocking(move || backup::verify(Path::new(&path))).await
}

#[tauri::command]
pub fn suggest_backup_name(
    services: State<'_, AppServices>,
    device_id: String,
    region: Option<FlashRegion>,
) -> Result<String> {
    let device = services.devices().get(&device_id)?;
    let info = device.chip_info.ok_or_else(|| Error::invalid_input("Identify the device before creating a backup."))?;
    let region = region.unwrap_or_else(|| FlashRegion::new(0, info.flash_size_bytes().unwrap_or(0)));
    Ok(backup::suggested_file_name(
        info.chip,
        info.mac_address.as_deref(),
        region.offset,
        region.length,
        &now_rfc3339(),
    ))
}

#[tauri::command]
pub async fn load_partition_table(
    path: String,
    flash_size: Option<u32>,
    table_offset: Option<u32>,
) -> Result<PartitionTableView> {
    blocking(move || {
        let path = Path::new(&path);
        fsutil::require_absolute(path)?;
        let (table, format) = partitions::load(path)?;
        Ok(partitions::view(&table, format, flash_size, table_offset.unwrap_or(partitions::DEFAULT_TABLE_OFFSET)))
    })
    .await
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExportResult {
    pub output_path: String,
    pub format: TableFormat,
    #[ts(type = "number")]
    pub size: u64,
    pub sha256: String,
}

/// Write a partition table in CSV or binary format.
#[tauri::command]
pub async fn export_partition_table(
    entries: Vec<PartitionEntry>,
    format: TableFormat,
    output_path: String,
) -> Result<ExportResult> {
    blocking(move || {
        let output = Path::new(&output_path);
        fsutil::validate_output(output)?;
        let table = partitions::from_entries(&entries)?;
        let bytes = match format {
            TableFormat::Binary => partitions::to_binary(&table)?,
            TableFormat::Csv => partitions::to_csv(&table)?.into_bytes(),
        };
        let sha256 = fsutil::write_atomic(output, &bytes)?;
        Ok(ExportResult { output_path, format, size: bytes.len() as u64, sha256 })
    })
    .await
}

#[tauri::command]
pub async fn preview_image(request: PreviewImageRequest) -> Result<ImageSummary> {
    blocking(move || image::preview(&request)).await
}

#[tauri::command]
pub async fn save_image(request: SaveImageRequest) -> Result<SaveImageResult> {
    blocking(move || image::save_image(&request)).await
}

#[tauri::command]
pub async fn start_monitor(
    services: State<'_, AppServices>,
    request: MonitorRequest,
    on_event: Channel<MonitorEvent>,
) -> Result<String> {
    let services = services.inner().clone();
    blocking(move || {
        let sink = Arc::new(move |event: MonitorEvent| {
            if let Err(err) = on_event.send(event) {
                log::debug!("Monitor event could not be delivered: {err}");
            }
        });
        services.start_monitor(&request, sink)
    })
    .await
}

#[tauri::command]
pub fn monitor_send(
    services: State<'_, AppServices>,
    monitor_id: String,
    text: String,
    line_ending: LineEnding,
) -> Result<()> {
    let data = monitor::encode_input(&text, line_ending)?;
    services.monitor_command(&monitor_id, MonitorCommand::Send(data))
}

#[tauri::command]
pub fn monitor_reset(services: State<'_, AppServices>, monitor_id: String) -> Result<()> {
    services.monitor_command(&monitor_id, MonitorCommand::Reset)
}

#[tauri::command]
pub async fn stop_monitor(services: State<'_, AppServices>, monitor_id: String) -> Result<()> {
    let services = services.inner().clone();
    blocking(move || services.stop_monitor(&monitor_id)).await
}

/// Upper bound for text exported from the user interface.
const MAX_TEXT_EXPORT: usize = 64 * 1024 * 1024;

#[tauri::command]
pub async fn save_text_file(path: String, contents: String) -> Result<()> {
    blocking(move || {
        if contents.len() > MAX_TEXT_EXPORT {
            return Err(Error::invalid_input("The text is too large to save."));
        }
        fsutil::write_atomic(Path::new(&path), contents.as_bytes()).map(|_| ())
    })
    .await
}

#[tauri::command]
pub fn get_settings(store: State<'_, SettingsStore>) -> Settings {
    store.get()
}

#[tauri::command]
pub async fn update_settings(store: State<'_, SettingsStore>, settings: Settings) -> Result<Settings> {
    let result = store.update(settings);
    if let Ok(saved) = &result {
        log::set_max_level(crate::log_filter(saved.log_level));
    }
    result
}

/// Open the folder containing the application log files.
#[tauri::command]
pub fn open_log_folder(app: tauri::AppHandle) -> Result<()> {
    use tauri::Manager;
    let dir = app
        .path()
        .app_log_dir()
        .map_err(|e| Error::new(ErrorKind::Io, "The log folder is unavailable.").with_detail(e.to_string()))?;
    std::fs::create_dir_all(&dir).map_err(|e| Error::file_io(&e, &dir, "create"))?;
    tauri_plugin_opener::open_path(&dir, None::<&str>)
        .map_err(|e| Error::new(ErrorKind::Io, "The log folder could not be opened.").with_detail(e.to_string()))
}

/// Show a file or folder in the system file manager.
#[tauri::command]
pub fn reveal_path(path: String) -> Result<()> {
    let path = Path::new(&path);
    fsutil::require_absolute(path)?;
    if !path.exists() {
        return Err(Error::new(ErrorKind::InvalidFile, "The file no longer exists."));
    }
    tauri_plugin_opener::reveal_item_in_dir(path)
        .map_err(|e| Error::new(ErrorKind::Io, "The file manager could not be opened.").with_detail(e.to_string()))
}
