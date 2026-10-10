import type { FlashFrequency } from '$lib/bindings/FlashFrequency';
import type { FlashMode } from '$lib/bindings/FlashMode';
import type { FlashSize } from '$lib/bindings/FlashSize';

export const BAUD_RATES = [115_200, 230_400, 460_800, 921_600, 1_500_000, 2_000_000];
export const MONITOR_BAUD_RATES = [9_600, 19_200, 38_400, 57_600, 74_880, 115_200, 230_400, 460_800, 921_600, 1_500_000, 2_000_000];

export const FLASH_MODES: { value: FlashMode; label: string }[] = [
  { value: 'qio', label: 'QIO' },
  { value: 'qout', label: 'QOUT' },
  { value: 'dio', label: 'DIO' },
  { value: 'dout', label: 'DOUT' },
];

export const FLASH_SIZES: FlashSize[] = ['256KB', '512KB', '1MB', '2MB', '4MB', '8MB', '16MB', '32MB', '64MB', '128MB', '256MB'];

export const FLASH_FREQUENCIES: FlashFrequency[] = [
  '12MHz',
  '15MHz',
  '16MHz',
  '20MHz',
  '24MHz',
  '26MHz',
  '30MHz',
  '40MHz',
  '48MHz',
  '60MHz',
  '80MHz',
];

export function flashSizeBytes(size: FlashSize): number {
  const value = parseInt(size, 10);
  return size.endsWith('KB') ? value * 1024 : value * 1024 * 1024;
}

/** Sentinel used by selects for "use the detected or default value". */
export const AUTO = 'auto';
