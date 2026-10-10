export type ViewId = 'dashboard' | 'flash' | 'backup' | 'erase' | 'monitor' | 'partitions' | 'image' | 'settings';

/** Request from another view to prefill the Flash view. */
export type FlashIntent =
  | { mode: 'elf'; path: string }
  | { mode: 'bin'; path: string; offset: number | null };

/** Request to prefill the serial monitor after flashing. */
export interface MonitorIntent {
  deviceId: string;
  elfPath: string | null;
  autoStart: boolean;
}

class Navigation {
  view = $state<ViewId>('dashboard');
  flashIntent = $state<FlashIntent | null>(null);
  monitorIntent = $state<MonitorIntent | null>(null);

  go(view: ViewId): void {
    this.view = view;
  }

  openFlash(intent: FlashIntent): void {
    this.flashIntent = intent;
    this.view = 'flash';
  }

  openMonitor(intent: MonitorIntent): void {
    this.monitorIntent = intent;
    this.view = 'monitor';
  }
}

export const nav = new Navigation();
