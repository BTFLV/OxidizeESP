/**
 * Typed wrappers around the Tauri commands exposed by the Rust backend.
 * Request and response types are generated from Rust (see `bindings/`).
 */
import { Channel, invoke } from '@tauri-apps/api/core';
import type { AppInfo } from '$lib/bindings/AppInfo';
import type { BackupResult } from '$lib/bindings/BackupResult';
import type { BackupVerification } from '$lib/bindings/BackupVerification';
import type { ChecksumRequest } from '$lib/bindings/ChecksumRequest';
import type { ChecksumResult } from '$lib/bindings/ChecksumResult';
import type { ChipDescriptor } from '$lib/bindings/ChipDescriptor';
import type { ChipInfo } from '$lib/bindings/ChipInfo';
import type { ConnectionOptions } from '$lib/bindings/ConnectionOptions';
import type { DeviceSnapshot } from '$lib/bindings/DeviceSnapshot';
import type { ElfInfo } from '$lib/bindings/ElfInfo';
import type { EraseRequest } from '$lib/bindings/EraseRequest';
import type { EraseResult } from '$lib/bindings/EraseResult';
import type { ExportResult } from '$lib/bindings/ExportResult';
import type { FileInfo } from '$lib/bindings/FileInfo';
import type { FlashElfRequest } from '$lib/bindings/FlashElfRequest';
import type { FlashRegion } from '$lib/bindings/FlashRegion';
import type { FlashResult } from '$lib/bindings/FlashResult';
import type { ImageSummary } from '$lib/bindings/ImageSummary';
import type { JobEvent } from '$lib/bindings/JobEvent';
import type { LineEnding } from '$lib/bindings/LineEnding';
import type { LocalChecksum } from '$lib/bindings/LocalChecksum';
import type { MonitorEvent } from '$lib/bindings/MonitorEvent';
import type { MonitorRequest } from '$lib/bindings/MonitorRequest';
import type { PartitionEntry } from '$lib/bindings/PartitionEntry';
import type { PartitionTableView } from '$lib/bindings/PartitionTableView';
import type { PreviewImageRequest } from '$lib/bindings/PreviewImageRequest';
import type { ReadFlashRequest } from '$lib/bindings/ReadFlashRequest';
import type { ReadPartitionTableRequest } from '$lib/bindings/ReadPartitionTableRequest';
import type { SaveImageRequest } from '$lib/bindings/SaveImageRequest';
import type { SaveImageResult } from '$lib/bindings/SaveImageResult';
import type { SerialPortDescriptor } from '$lib/bindings/SerialPortDescriptor';
import type { Settings } from '$lib/bindings/Settings';
import type { TableFormat } from '$lib/bindings/TableFormat';
import type { WriteBinRequest } from '$lib/bindings/WriteBinRequest';
import { toAppError } from '$lib/utils/errors';

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw toAppError(error);
  }
}

/** Callback receiving job events from a hardware operation. */
export type JobListener = (event: JobEvent) => void;

function jobChannel(listener: JobListener): Channel<JobEvent> {
  const channel = new Channel<JobEvent>();
  channel.onmessage = listener;
  return channel;
}

export const api = {
  appInfo: () => call<AppInfo>('app_info'),
  chipDescriptors: () => call<ChipDescriptor[]>('chip_descriptors'),

  listPorts: () => call<SerialPortDescriptor[]>('list_ports'),
  listDevices: () => call<DeviceSnapshot[]>('list_devices'),
  openDevice: (portName: string) => call<DeviceSnapshot>('open_device', { portName }),
  closeDevice: (deviceId: string) => call<void>('close_device', { deviceId }),

  probeDevice: (deviceId: string, connection: ConnectionOptions, onEvent: JobListener) =>
    call<ChipInfo>('probe_device', { deviceId, connection, onEvent: jobChannel(onEvent) }),
  resetDevice: (deviceId: string, onEvent: JobListener) =>
    call<void>('reset_device', { deviceId, onEvent: jobChannel(onEvent) }),
  holdInReset: (deviceId: string, connection: ConnectionOptions, onEvent: JobListener) =>
    call<ChipInfo>('hold_in_reset', { deviceId, connection, onEvent: jobChannel(onEvent) }),

  flashElf: (request: FlashElfRequest, onEvent: JobListener) =>
    call<FlashResult>('flash_elf', { request, onEvent: jobChannel(onEvent) }),
  writeBin: (request: WriteBinRequest, onEvent: JobListener) =>
    call<FlashResult>('write_bin', { request, onEvent: jobChannel(onEvent) }),
  readFlash: (request: ReadFlashRequest, onEvent: JobListener) =>
    call<BackupResult>('read_flash', { request, onEvent: jobChannel(onEvent) }),
  readPartitionTable: (request: ReadPartitionTableRequest, onEvent: JobListener) =>
    call<PartitionTableView>('read_partition_table', { request, onEvent: jobChannel(onEvent) }),
  erase: (request: EraseRequest, onEvent: JobListener) =>
    call<EraseResult>('erase', { request, onEvent: jobChannel(onEvent) }),
  checksumMd5: (request: ChecksumRequest, onEvent: JobListener) =>
    call<ChecksumResult>('checksum_md5', { request, onEvent: jobChannel(onEvent) }),
  cancelJob: (jobId: string) => call<void>('cancel_job', { jobId }),

  inspectFile: (path: string) => call<FileInfo>('inspect_file', { path }),
  inspectElf: (path: string) => call<ElfInfo>('inspect_elf', { path }),
  fileMd5: (path: string, offset: number, length: number | null) =>
    call<LocalChecksum>('file_md5', { path, offset, length }),
  verifyBackup: (path: string) => call<BackupVerification>('verify_backup', { path }),
  suggestBackupName: (deviceId: string, region: FlashRegion | null) =>
    call<string>('suggest_backup_name', { deviceId, region }),

  loadPartitionTable: (path: string, flashSize: number | null, tableOffset: number | null) =>
    call<PartitionTableView>('load_partition_table', { path, flashSize, tableOffset }),
  exportPartitionTable: (entries: PartitionEntry[], format: TableFormat, outputPath: string) =>
    call<ExportResult>('export_partition_table', { entries, format, outputPath }),

  previewImage: (request: PreviewImageRequest) => call<ImageSummary>('preview_image', { request }),
  saveImage: (request: SaveImageRequest) => call<SaveImageResult>('save_image', { request }),

  startMonitor: (request: MonitorRequest, onEvent: (event: MonitorEvent) => void) => {
    const channel = new Channel<MonitorEvent>();
    channel.onmessage = onEvent;
    return call<string>('start_monitor', { request, onEvent: channel });
  },
  monitorSend: (monitorId: string, text: string, lineEnding: LineEnding) =>
    call<void>('monitor_send', { monitorId, text, lineEnding }),
  monitorReset: (monitorId: string) => call<void>('monitor_reset', { monitorId }),
  stopMonitor: (monitorId: string) => call<void>('stop_monitor', { monitorId }),
  saveTextFile: (path: string, contents: string) => call<void>('save_text_file', { path, contents }),

  getSettings: () => call<Settings>('get_settings'),
  updateSettings: (settings: Settings) => call<Settings>('update_settings', { settings }),

  revealPath: (path: string) => call<void>('reveal_path', { path }),
  openLogFolder: () => call<void>('open_log_folder'),
};
