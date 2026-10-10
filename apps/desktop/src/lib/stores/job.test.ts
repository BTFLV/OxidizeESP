import { describe, expect, it } from 'vitest';
import { completeJob, failJob, initialJob, reduceJob, startJob } from './job';

describe('job reducer', () => {
  it('tracks a successful operation', () => {
    let s = startJob(0);
    s = reduceJob(s, { event: 'started', jobId: 'j', kind: 'readFlash', cancellable: true }, 0);
    expect(s.cancellable).toBe(true);
    s = reduceJob(s, { event: 'stage', stage: 'reading', message: 'Reading' }, 10);
    s = reduceJob(s, { event: 'progress', current: 0, total: 1000, unit: 'bytes', overall: 0, detail: null }, 20);
    s = reduceJob(s, { event: 'progress', current: 500, total: 1000, unit: 'bytes', overall: 0.5, detail: null }, 1020);
    expect(s.progress?.overall).toBe(0.5);
    expect(s.bytesPerSecond).toBe(500);
    s = reduceJob(s, { event: 'completed', message: 'Done' }, 2000);
    expect(s.phase).toBe('completed');
    expect(s.progress?.overall).toBe(1);
    expect(s.log.map((l) => l.level)).toEqual(['stage', 'success']);
  });

  it('ignores events after a terminal state', () => {
    let s = reduceJob(startJob(0), { event: 'completed', message: 'Done' }, 1);
    s = reduceJob(s, { event: 'progress', current: 1, total: 2, unit: 'bytes', overall: 0.5, detail: null }, 2);
    expect(s.phase).toBe('completed');
    expect(s.progress).toBeNull();
  });

  it('records failures with inconsistent flash', () => {
    const error = { kind: 'flashOperation' as const, message: 'Write failed', hint: null, detail: null };
    const s = reduceJob(startJob(0), { event: 'failed', error, flashMayBeInconsistent: true }, 5);
    expect(s.phase).toBe('failed');
    expect(s.flashMayBeInconsistent).toBe(true);
  });

  it('handles errors that occur before the event stream', () => {
    const busy = { kind: 'portBusy' as const, message: 'Busy', hint: null, detail: null };
    expect(failJob(startJob(0), busy, 1).phase).toBe('failed');
    const cancelled = { kind: 'cancelled' as const, message: 'Cancelled', hint: null, detail: null };
    expect(failJob(startJob(0), cancelled, 1).phase).toBe('cancelled');
    const done = completeJob(startJob(0), 'ok', 1);
    expect(failJob(done, busy, 2).phase).toBe('completed');
  });

  it('starts idle', () => {
    expect(initialJob().phase).toBe('idle');
  });
});
