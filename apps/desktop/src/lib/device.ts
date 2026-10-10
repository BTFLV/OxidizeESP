import type { BadgeTone } from '$lib/components/ui/Badge.svelte';
import type { DeviceSnapshot } from '$lib/bindings/DeviceSnapshot';
import type { OperationKind } from '$lib/bindings/OperationKind';
import type { SerialPortDescriptor } from '$lib/bindings/SerialPortDescriptor';
import { formatVidPid } from '$lib/utils/format';

const OPERATION_LABELS: Record<OperationKind, string> = {
  probe: 'Identifying',
  flashElf: 'Flashing',
  writeBin: 'Writing',
  readFlash: 'Reading flash',
  readPartitionTable: 'Reading partition table',
  eraseFlash: 'Erasing',
  eraseRegion: 'Erasing',
  erasePartitions: 'Erasing',
  checksumMd5: 'Calculating checksum',
  reset: 'Resetting',
  holdInReset: 'Entering bootloader',
};

export function statusLabel(device: DeviceSnapshot | null): string {
  if (!device) return 'No device';
  switch (device.status) {
    case 'disconnected':
      return 'Disconnected';
    case 'unverified':
      return 'Not identified';
    case 'ready':
      return 'Ready';
    case 'busy':
      return device.activity.type === 'monitoring'
        ? 'Monitoring'
        : device.activity.type === 'operation'
          ? OPERATION_LABELS[device.activity.kind]
          : 'Busy';
  }
}

export function statusTone(device: DeviceSnapshot | null): BadgeTone {
  if (!device) return 'neutral';
  switch (device.status) {
    case 'disconnected':
      return 'danger';
    case 'unverified':
      return 'warning';
    case 'ready':
      return 'success';
    case 'busy':
      return 'info';
  }
}

export function deviceTitle(device: DeviceSnapshot): string {
  return device.chipInfo?.chipName ?? device.port.usb?.product ?? 'Unidentified device';
}

/**
 * Reason why a hardware operation cannot start on `device`, or null when it
 * can. Identification is required for operations that rely on verified chip
 * information.
 */
export function deviceBlocker(device: DeviceSnapshot | null, requireIdentified = true): string | null {
  if (!device) return 'Connect a device first.';
  if (device.status === 'disconnected') return `The device on ${device.port.name} is disconnected.`;
  if (device.activity.type === 'monitoring') return 'The serial monitor is using this device. Stop it first.';
  if (device.activity.type === 'operation') return `The device is busy (${OPERATION_LABELS[device.activity.kind].toLowerCase()}).`;
  if (requireIdentified && !device.chipInfo) return 'Identify the device first.';
  return null;
}

export function portDescription(port: SerialPortDescriptor): string {
  const parts: string[] = [];
  if (port.usb) {
    const name = port.usb.product ?? port.bridge ?? port.usb.manufacturer;
    if (name) parts.push(name);
    parts.push(formatVidPid(port.usb.vid, port.usb.pid));
  } else {
    parts.push(port.kind === 'pci' ? 'PCI serial port' : 'Serial port');
  }
  return parts.join(' · ');
}
