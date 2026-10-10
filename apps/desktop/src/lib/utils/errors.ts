import type { Error as AppError } from '$lib/bindings/Error';
import type { ErrorKind } from '$lib/bindings/ErrorKind';

export type { AppError };

const KINDS: ReadonlySet<string> = new Set<ErrorKind>([
  'connection',
  'disconnected',
  'portBusy',
  'permission',
  'unsupportedHardware',
  'deviceMismatch',
  'invalidFile',
  'incompatibleFirmware',
  'flashOperation',
  'verification',
  'invalidInput',
  'cancelled',
  'io',
  'unsupported',
  'internal',
]);

export function isAppError(value: unknown): value is AppError {
  return (
    typeof value === 'object' &&
    value !== null &&
    'kind' in value &&
    'message' in value &&
    typeof (value as { message: unknown }).message === 'string' &&
    KINDS.has(String((value as { kind: unknown }).kind))
  );
}

/** Normalize anything thrown by an IPC call into an application error. */
export function toAppError(value: unknown): AppError {
  if (isAppError(value)) return value;
  const message =
    typeof value === 'string' ? value : value instanceof Error ? value.message : 'An unexpected error occurred.';
  return { kind: 'internal', message, hint: null, detail: null };
}

const TITLES: Record<ErrorKind, string> = {
  connection: 'Connection failed',
  disconnected: 'Device disconnected',
  portBusy: 'Port in use',
  permission: 'Permission denied',
  unsupportedHardware: 'Unsupported hardware',
  deviceMismatch: 'Different device detected',
  invalidFile: 'Invalid file',
  incompatibleFirmware: 'Incompatible firmware',
  flashOperation: 'Flash operation failed',
  verification: 'Verification failed',
  invalidInput: 'Invalid input',
  cancelled: 'Cancelled',
  io: 'File system error',
  unsupported: 'Not supported',
  internal: 'Unexpected error',
};

export function errorTitle(error: AppError): string {
  return TITLES[error.kind];
}
