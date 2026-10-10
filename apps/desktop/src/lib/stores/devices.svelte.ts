import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { api } from '$lib/api';
import type { ConnectionOptions } from '$lib/bindings/ConnectionOptions';
import type { DeviceSnapshot } from '$lib/bindings/DeviceSnapshot';
import type { SerialPortDescriptor } from '$lib/bindings/SerialPortDescriptor';
import { type AppError, toAppError } from '$lib/utils/errors';
import { JobRunner } from './job.svelte';
import { settings } from './settings.svelte';

/** Port enumeration interval. Enumeration never opens a port. */
const POLL_INTERVAL_MS = 2000;

/** Pick the port to preselect according to the user's preference. */
export function choosePreferredPort(
  ports: SerialPortDescriptor[],
  preference: 'rememberLast' | 'autoSelectSingle' | 'manual',
  lastPort: string | null,
): string | null {
  if (preference === 'manual') return null;
  if (preference === 'rememberLast' && lastPort && ports.some((p) => p.name === lastPort)) return lastPort;
  const likely = ports.filter((p) => p.hint !== 'other');
  return likely.length === 1 ? likely[0]!.name : null;
}

class DeviceState {
  ports = $state<SerialPortDescriptor[]>([]);
  devices = $state<DeviceSnapshot[]>([]);
  activeId = $state<string | null>(null);
  selectedPort = $state<string | null>(null);
  portsError = $state<AppError | null>(null);
  refreshing = $state(false);
  initialized = $state(false);
  /** Runner for connect/identify operations started from the shell. */
  connectJob = new JobRunner();
  #timer: ReturnType<typeof setInterval> | null = null;
  #unlisten: UnlistenFn | null = null;
  #listeners = new Set<(devices: DeviceSnapshot[]) => void>();

  active = $derived(this.devices.find((d) => d.id === this.activeId) ?? null);

  async init(): Promise<void> {
    if (this.initialized) return;
    this.initialized = true;
    this.#unlisten = await listen<DeviceSnapshot[]>('devices-changed', (event) => this.#setDevices(event.payload));
    this.#setDevices(await api.listDevices().catch(() => []));
    await this.refreshPorts();
    const s = settings.value;
    this.selectedPort ??= choosePreferredPort(this.ports, s.portSelection, s.lastPort);
    if (this.selectedPort) this.activeId ??= this.sessionForPort(this.selectedPort)?.id ?? null;
    this.#timer = setInterval(() => {
      if (document.visibilityState === 'visible') void this.refreshPorts(true);
    }, POLL_INTERVAL_MS);
  }

  dispose(): void {
    if (this.#timer) clearInterval(this.#timer);
    this.#unlisten?.();
    this.initialized = false;
  }

  /** Subscribe to device list changes. */
  onChange(listener: (devices: DeviceSnapshot[]) => void): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  #setDevices(devices: DeviceSnapshot[]): void {
    this.devices = devices;
    if (this.activeId && !devices.some((d) => d.id === this.activeId)) {
      this.activeId = devices[0]?.id ?? null;
    }
    for (const listener of this.#listeners) listener(devices);
  }

  async refreshPorts(silent = false): Promise<void> {
    if (!silent) this.refreshing = true;
    try {
      this.ports = await api.listPorts();
      this.portsError = null;
      if (this.selectedPort && !this.ports.some((p) => p.name === this.selectedPort)) {
        const session = this.devices.find((d) => d.port.name === this.selectedPort);
        if (!session) this.selectedPort = null;
      }
    } catch (error) {
      this.portsError = toAppError(error);
    } finally {
      if (!silent) this.refreshing = false;
    }
  }

  connectionOptions(overrides: Partial<ConnectionOptions> = {}): ConnectionOptions {
    return {
      baud: settings.value.flashBaud,
      useStub: settings.value.useStub,
      resetBefore: 'defaultReset',
      resetAfter: 'hardReset',
      ...overrides,
    };
  }

  sessionForPort(portName: string): DeviceSnapshot | null {
    return this.devices.find((d) => d.port.name === portName) ?? null;
  }

  /** Open a session for a port without communicating with the device. */
  async open(portName: string): Promise<DeviceSnapshot> {
    const device = await api.openDevice(portName);
    this.activeId = device.id;
    this.selectedPort = portName;
    if (settings.value.lastPort !== portName) {
      void settings.update((s) => {
        s.lastPort = portName;
      });
    }
    return device;
  }

  /** Open a session for a port and identify the chip. */
  async connect(portName: string): Promise<void> {
    let device: DeviceSnapshot;
    try {
      device = await this.open(portName);
    } catch (error) {
      this.connectJob.state = { ...this.connectJob.state, phase: 'failed', error: toAppError(error) };
      return;
    }
    await this.identify(device.id);
  }

  async identify(deviceId: string): Promise<void> {
    await this.connectJob.run({ label: 'Identifying device', deviceId, kind: 'probe' }, (listener) =>
      api.probeDevice(deviceId, this.connectionOptions(), listener),
    );
  }

  async disconnect(deviceId: string): Promise<AppError | null> {
    try {
      await api.closeDevice(deviceId);
      return null;
    } catch (error) {
      return toAppError(error);
    }
  }
}

export const devices = new DeviceState();
