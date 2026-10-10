import type { Component } from 'svelte';
import { Cpu, Eraser, FileCog, HardDriveDownload, LayoutDashboard, Settings, TableProperties, Terminal, Zap } from '$lib/icons';
import type { ViewId } from '$lib/stores/nav.svelte';

export interface ViewDefinition {
  id: ViewId;
  label: string;
  icon: Component;
}

export const VIEWS: ViewDefinition[] = [
  { id: 'dashboard', label: 'Dashboard', icon: LayoutDashboard },
  { id: 'flash', label: 'Flash', icon: Zap },
  { id: 'backup', label: 'Backup', icon: HardDriveDownload },
  { id: 'erase', label: 'Erase', icon: Eraser },
  { id: 'monitor', label: 'Serial Monitor', icon: Terminal },
  { id: 'partitions', label: 'Partitions', icon: TableProperties },
  { id: 'image', label: 'Image Tools', icon: FileCog },
  { id: 'settings', label: 'Settings', icon: Settings },
];

export const DeviceIcon = Cpu;
