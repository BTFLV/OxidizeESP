import { api } from '$lib/api';
import type { DeviceSnapshot } from '$lib/bindings/DeviceSnapshot';
import type { LineEnding } from '$lib/bindings/LineEnding';
import type { MonitorEvent } from '$lib/bindings/MonitorEvent';
import type { MonitorLine } from '$lib/bindings/MonitorLine';
import { type AppError, toAppError } from '$lib/utils/errors';
import { RingBuffer } from '$lib/utils/ringbuffer';
import { devices } from './devices.svelte';
import { settings } from './settings.svelte';

export type MonitorStatus = 'stopped' | 'starting' | 'running' | 'stopping' | 'waiting';

export interface MonitorStartOptions {
  deviceId: string;
  baud: number;
  elfPath: string | null;
  logPath: string | null;
  resetOnStart: boolean;
}

/**
 * Serial monitor session shown in the Monitor view. Received lines are kept
 * in a bounded ring buffer; the view re-renders through `version`, which is
 * bumped at most once per animation frame.
 */
class MonitorState {
  status = $state<MonitorStatus>('stopped');
  deviceId = $state<string | null>(null);
  port = $state('');
  symbolsLoaded = $state(false);
  error = $state<AppError | null>(null);
  /** Incremented whenever the buffer changes. */
  version = $state(0);
  /** Total number of lines received in this session. */
  received = $state(0);
  paused = $state(false);
  options = $state<MonitorStartOptions | null>(null);

  #buffer = new RingBuffer<MonitorLine>(settings.value.monitor.maxLines);
  #monitorId: string | null = null;
  #frame: number | null = null;
  #unsubscribe: (() => void) | null = null;

  get buffer(): RingBuffer<MonitorLine> {
    return this.#buffer;
  }

  get active(): boolean {
    return this.status === 'running' || this.status === 'starting';
  }

  setCapacity(lines: number): void {
    if (lines !== this.#buffer.capacity) {
      this.#buffer = this.#buffer.resized(lines);
      this.#bump();
    }
  }

  #bump(): void {
    if (this.#frame !== null) return;
    const schedule = typeof requestAnimationFrame === 'function' ? requestAnimationFrame : (cb: () => void) => setTimeout(cb, 16);
    this.#frame = schedule(() => {
      this.#frame = null;
      this.version++;
    }) as unknown as number;
  }

  #handle = (event: MonitorEvent): void => {
    switch (event.event) {
      case 'started':
        this.status = 'running';
        this.port = event.port;
        this.symbolsLoaded = event.symbolsLoaded;
        break;
      case 'lines':
        this.#buffer.pushAll(event.lines);
        this.received += event.lines.length;
        this.#bump();
        break;
      case 'stopped':
        this.#monitorId = null;
        if (event.error) {
          this.error = event.error;
          if (event.error.kind === 'disconnected' && settings.value.monitor.autoReconnect && this.options) {
            this.status = 'waiting';
            this.#watchForReconnect();
            break;
          }
        }
        this.status = 'stopped';
        break;
    }
  };

  async start(options: MonitorStartOptions): Promise<AppError | null> {
    if (this.active) return null;
    this.#stopWatching();
    this.options = options;
    this.deviceId = options.deviceId;
    this.error = null;
    this.status = 'starting';
    try {
      this.#monitorId = await api.startMonitor(
        {
          deviceId: options.deviceId,
          baud: options.baud,
          elfPath: options.elfPath,
          logPath: options.logPath,
          resetOnStart: options.resetOnStart,
        },
        this.#handle,
      );
      if (this.status === 'starting') this.status = 'running';
      return null;
    } catch (error) {
      const appError = toAppError(error);
      this.error = appError;
      this.status = 'stopped';
      return appError;
    }
  }

  async stop(): Promise<void> {
    this.#stopWatching();
    if (this.status === 'waiting') {
      this.status = 'stopped';
      return;
    }
    const id = this.#monitorId;
    if (!id) {
      this.status = 'stopped';
      return;
    }
    this.status = 'stopping';
    try {
      await api.stopMonitor(id);
    } catch (error) {
      this.error = toAppError(error);
    } finally {
      this.#monitorId = null;
      this.status = 'stopped';
    }
  }

  async reset(): Promise<AppError | null> {
    if (!this.#monitorId) return null;
    try {
      await api.monitorReset(this.#monitorId);
      return null;
    } catch (error) {
      return toAppError(error);
    }
  }

  async send(text: string, lineEnding: LineEnding): Promise<AppError | null> {
    if (!this.#monitorId) return null;
    try {
      await api.monitorSend(this.#monitorId, text, lineEnding);
      return null;
    } catch (error) {
      return toAppError(error);
    }
  }

  clear(): void {
    this.#buffer.clear();
    this.received = 0;
    this.#bump();
  }

  #watchForReconnect(): void {
    this.#stopWatching();
    this.#unsubscribe = devices.onChange((list: DeviceSnapshot[]) => {
      const options = this.options;
      if (this.status !== 'waiting' || !options) return;
      const device = list.find((d) => d.id === options.deviceId);
      if (!device) {
        this.#stopWatching();
        this.status = 'stopped';
        return;
      }
      if (device.status !== 'disconnected' && device.activity.type === 'idle') {
        this.#stopWatching();
        this.status = 'stopped';
        void this.start({ ...options, resetOnStart: false });
      }
    });
  }

  #stopWatching(): void {
    this.#unsubscribe?.();
    this.#unsubscribe = null;
  }
}

export const monitor = new MonitorState();
