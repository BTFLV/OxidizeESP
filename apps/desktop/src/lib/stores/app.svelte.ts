import { api } from '$lib/api';
import type { AppInfo } from '$lib/bindings/AppInfo';
import type { ChipDescriptor } from '$lib/bindings/ChipDescriptor';
import type { Chip } from '$lib/bindings/Chip';

class AppState {
  info = $state<AppInfo | null>(null);
  chips = $state<ChipDescriptor[]>([]);

  get isMac(): boolean {
    return this.info?.os === 'macos';
  }

  get modKey(): string {
    return this.isMac ? '⌘' : 'Ctrl+';
  }

  async load(): Promise<void> {
    const [info, chips] = await Promise.all([api.appInfo(), api.chipDescriptors()]);
    this.info = info;
    this.chips = chips;
  }

  chip(chip: Chip | null | undefined): ChipDescriptor | null {
    return this.chips.find((c) => c.chip === chip) ?? null;
  }
}

export const app = new AppState();
