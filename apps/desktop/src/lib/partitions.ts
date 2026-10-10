import type { PartitionEntry } from '$lib/bindings/PartitionEntry';

export type PartitionCategory = 'app' | 'nvs' | 'ota' | 'filesystem' | 'data' | 'custom';

export function categorize(entry: PartitionEntry): PartitionCategory {
  if (entry.typeName === 'app') return 'app';
  if (entry.typeName !== 'data') return 'custom';
  if (entry.subtypeName === 'nvs' || entry.subtypeName === 'nvs_keys') return 'nvs';
  if (entry.subtypeName === 'ota') return 'ota';
  if (['fat', 'spiffs', 'littlefs'].includes(entry.subtypeName)) return 'filesystem';
  return 'data';
}

export const CATEGORY_LABELS: Record<PartitionCategory, string> = {
  app: 'Application',
  nvs: 'NVS',
  ota: 'OTA data',
  filesystem: 'File system',
  data: 'Other data',
  custom: 'Custom type',
};

export const CATEGORY_COLORS: Record<PartitionCategory, string> = {
  app: 'bg-accent',
  nvs: 'bg-info',
  ota: 'bg-warning',
  filesystem: 'bg-success',
  data: 'bg-fg-subtle',
  custom: 'bg-danger',
};

export interface MapSegment {
  key: string;
  label: string;
  offset: number;
  size: number;
  kind: 'reserved' | PartitionCategory;
}

/** Build the segments of a flash memory map. */
export function buildMap(entries: PartitionEntry[], tableOffset: number): MapSegment[] {
  const segments: MapSegment[] = [
    { key: 'bootloader', label: 'Bootloader area', offset: 0, size: tableOffset, kind: 'reserved' },
    { key: 'table', label: 'Partition table', offset: tableOffset, size: 0x1000, kind: 'reserved' },
  ];
  for (const e of [...entries].sort((a, b) => a.offset - b.offset)) {
    segments.push({ key: `p-${e.name}`, label: e.name, offset: e.offset, size: e.size, kind: categorize(e) });
  }
  return segments;
}

/** Total size shown by the map: the flash size, or the end of the layout. */
export function mapExtent(entries: PartitionEntry[], flashSize: number | null, tableOffset: number): number {
  const end = Math.max(tableOffset + 0x1000, ...entries.map((e) => e.offset + e.size));
  return flashSize && flashSize >= end ? flashSize : end;
}
