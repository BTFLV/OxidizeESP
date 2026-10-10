import { describe, expect, it } from 'vitest';
import { errorTitle, isAppError, toAppError } from './errors';

describe('errors', () => {
  it('passes through backend errors', () => {
    const error = { kind: 'portBusy', message: 'In use', hint: 'Close it', detail: null };
    expect(isAppError(error)).toBe(true);
    expect(toAppError(error)).toBe(error);
    expect(errorTitle(toAppError(error))).toBe('Port in use');
  });

  it('normalizes unknown values', () => {
    expect(toAppError('boom')).toEqual({ kind: 'internal', message: 'boom', hint: null, detail: null });
    expect(toAppError(new Error('x')).message).toBe('x');
    expect(toAppError({ kind: 'nope', message: 'm' }).kind).toBe('internal');
  });
});
