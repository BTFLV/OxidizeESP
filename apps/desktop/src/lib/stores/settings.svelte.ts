import { api } from '$lib/api';
import type { Settings } from '$lib/bindings/Settings';
import { type AppError, toAppError } from '$lib/utils/errors';

/** Mirrors the Rust defaults so the interface renders before settings load. */
export const DEFAULT_SETTINGS: Settings = {
  appearance: 'dark',
  flashBaud: 460_800,
  portSelection: 'rememberLast',
  lastPort: null,
  firmwareDir: null,
  backupDir: null,
  useStub: true,
  verifyAfterFlash: true,
  showAdvanced: false,
  logLevel: 'info',
  monitor: {
    baud: 115_200,
    maxLines: 10_000,
    showTimestamps: true,
    autoscroll: true,
    lineEnding: 'lf',
    resetOnStart: false,
    autoReconnect: true,
    decodeAddresses: true,
  },
};

class SettingsState {
  value = $state<Settings>(structuredClone(DEFAULT_SETTINGS));
  loaded = $state(false);
  error = $state<AppError | null>(null);

  async load(): Promise<void> {
    try {
      this.value = await api.getSettings();
      this.error = null;
    } catch (error) {
      this.error = toAppError(error);
    } finally {
      this.loaded = true;
    }
  }

  /** Persist a change. Returns the error if the backend rejected it. */
  async update(change: (draft: Settings) => void): Promise<AppError | null> {
    const draft = $state.snapshot(this.value) as Settings;
    change(draft);
    try {
      this.value = await api.updateSettings(draft);
      this.error = null;
      return null;
    } catch (error) {
      const appError = toAppError(error);
      this.error = appError;
      return appError;
    }
  }
}

export const settings = new SettingsState();

/** Resolve the effective theme for the current appearance setting. */
export function resolveTheme(appearance: Settings['appearance'], prefersDark: boolean): 'dark' | 'light' {
  if (appearance === 'system') return prefersDark ? 'dark' : 'light';
  return appearance;
}
