import type { JobEvent } from '$lib/bindings/JobEvent';
import type { OperationKind } from '$lib/bindings/OperationKind';
import type { ProgressUnit } from '$lib/bindings/ProgressUnit';
import type { Stage } from '$lib/bindings/Stage';
import type { AppError } from '$lib/utils/errors';

export type JobPhase = 'idle' | 'running' | 'completed' | 'failed' | 'cancelled';

export interface JobLogEntry {
  time: number;
  level: 'info' | 'warning' | 'stage' | 'error' | 'success';
  message: string;
}

export interface JobProgress {
  overall: number;
  current: number;
  total: number;
  unit: ProgressUnit;
  detail: string | null;
}

export interface JobState {
  phase: JobPhase;
  jobId: string | null;
  kind: OperationKind | null;
  cancellable: boolean;
  cancelRequested: boolean;
  stage: Stage | null;
  stageMessage: string;
  progress: JobProgress | null;
  /** Byte throughput derived from byte-based progress events. */
  bytesPerSecond: number | null;
  log: JobLogEntry[];
  startedAt: number | null;
  finishedAt: number | null;
  message: string | null;
  error: AppError | null;
  flashMayBeInconsistent: boolean;
  /** Internal: first byte-progress sample used for the rate estimate. */
  rateOrigin: { time: number; bytes: number } | null;
}

export const MAX_LOG_ENTRIES = 500;

export function initialJob(): JobState {
  return {
    phase: 'idle',
    jobId: null,
    kind: null,
    cancellable: false,
    cancelRequested: false,
    stage: null,
    stageMessage: '',
    progress: null,
    bytesPerSecond: null,
    log: [],
    startedAt: null,
    finishedAt: null,
    message: null,
    error: null,
    flashMayBeInconsistent: false,
    rateOrigin: null,
  };
}

/** State at the start of a new job. */
export function startJob(now: number): JobState {
  return { ...initialJob(), phase: 'running', startedAt: now };
}

function appendLog(log: JobLogEntry[], entry: JobLogEntry): JobLogEntry[] {
  const next = log.length >= MAX_LOG_ENTRIES ? log.slice(log.length - MAX_LOG_ENTRIES + 1) : log.slice();
  next.push(entry);
  return next;
}

export function isTerminal(phase: JobPhase): boolean {
  return phase === 'completed' || phase === 'failed' || phase === 'cancelled';
}

/** Apply a backend event. Events after a terminal state are ignored. */
export function reduceJob(state: JobState, event: JobEvent, now: number): JobState {
  if (isTerminal(state.phase) && event.event !== 'log') return state;
  switch (event.event) {
    case 'started':
      return { ...state, phase: 'running', jobId: event.jobId, kind: event.kind, cancellable: event.cancellable };
    case 'stage':
      return {
        ...state,
        stage: event.stage,
        stageMessage: event.message,
        // Progress belongs to a step; a new stage without progress clears it.
        progress: event.stage === 'verifying' ? state.progress : null,
        rateOrigin: null,
        bytesPerSecond: event.stage === 'verifying' ? state.bytesPerSecond : null,
        log: appendLog(state.log, { time: now, level: 'stage', message: event.message }),
      };
    case 'progress': {
      let rateOrigin = state.rateOrigin;
      let bytesPerSecond = state.bytesPerSecond;
      if (event.unit === 'bytes') {
        if (!rateOrigin || event.current < rateOrigin.bytes) {
          rateOrigin = { time: now, bytes: event.current };
        } else if (now - rateOrigin.time >= 500) {
          bytesPerSecond = ((event.current - rateOrigin.bytes) * 1000) / (now - rateOrigin.time);
        }
      }
      return {
        ...state,
        progress: {
          overall: event.overall,
          current: event.current,
          total: event.total,
          unit: event.unit,
          detail: event.detail,
        },
        rateOrigin,
        bytesPerSecond,
      };
    }
    case 'log':
      return {
        ...state,
        log: appendLog(state.log, { time: now, level: event.level === 'warning' ? 'warning' : 'info', message: event.message }),
      };
    case 'completed':
      return {
        ...state,
        phase: 'completed',
        finishedAt: now,
        message: event.message,
        progress: state.progress ? { ...state.progress, overall: 1 } : null,
        log: appendLog(state.log, { time: now, level: 'success', message: event.message }),
      };
    case 'failed':
      return {
        ...state,
        phase: 'failed',
        finishedAt: now,
        error: event.error,
        flashMayBeInconsistent: event.flashMayBeInconsistent,
        log: appendLog(state.log, { time: now, level: 'error', message: event.error.message }),
      };
    case 'cancelled':
      return {
        ...state,
        phase: 'cancelled',
        finishedAt: now,
        message: event.message,
        flashMayBeInconsistent: event.flashMayBeInconsistent,
        log: appendLog(state.log, { time: now, level: 'warning', message: event.message }),
      };
  }
}

/** Apply a failure that happened before or outside the event stream. */
export function failJob(state: JobState, error: AppError, now: number): JobState {
  if (isTerminal(state.phase)) return state;
  if (error.kind === 'cancelled') {
    return { ...state, phase: 'cancelled', finishedAt: now, message: error.message };
  }
  return {
    ...state,
    phase: 'failed',
    finishedAt: now,
    error,
    log: appendLog(state.log, { time: now, level: 'error', message: error.message }),
  };
}

/** Mark the job completed when the call resolved but no terminal event arrived yet. */
export function completeJob(state: JobState, message: string, now: number): JobState {
  if (isTerminal(state.phase)) return state;
  return { ...state, phase: 'completed', finishedAt: now, message };
}
