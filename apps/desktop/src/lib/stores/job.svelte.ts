import { api, type JobListener } from '$lib/api';
import type { OperationKind } from '$lib/bindings/OperationKind';
import { toAppError } from '$lib/utils/errors';
import { activity } from './activity.svelte';
import { completeJob, failJob, initialJob, type JobState, reduceJob, startJob } from './job';

/**
 * Tracks one hardware operation for a view: progress, log, result and
 * cancellation. Each view owns its runner so results stay visible there.
 */
export class JobRunner {
  state = $state<JobState>(initialJob());
  label = $state('');
  deviceId = $state<string | null>(null);

  get running(): boolean {
    return this.state.phase === 'running';
  }

  /**
   * Run an operation. Resolves with the result, or `undefined` if the
   * operation failed or was cancelled (the error is kept in `state`).
   */
  async run<T>(
    options: { label: string; deviceId: string | null; kind?: OperationKind },
    start: (listener: JobListener) => Promise<T>,
  ): Promise<T | undefined> {
    if (this.running) return undefined;
    this.label = options.label;
    this.deviceId = options.deviceId;
    this.state = startJob(Date.now());
    if (options.kind) this.state = { ...this.state, kind: options.kind };
    const unregister = activity.register(this);
    try {
      const result = await start((event) => {
        this.state = reduceJob(this.state, event, Date.now());
      });
      this.state = completeJob(this.state, 'Completed', Date.now());
      return result;
    } catch (error) {
      this.state = failJob(this.state, toAppError(error), Date.now());
      return undefined;
    } finally {
      unregister();
    }
  }

  async cancel(): Promise<void> {
    const { jobId, cancellable } = this.state;
    if (!this.running || !jobId || !cancellable) return;
    this.state = { ...this.state, cancelRequested: true };
    try {
      await api.cancelJob(jobId);
    } catch (error) {
      const appError = toAppError(error);
      this.state = {
        ...this.state,
        cancelRequested: false,
        log: [...this.state.log, { time: Date.now(), level: 'warning', message: appError.message }],
      };
    }
  }

  reset(): void {
    if (!this.running) this.state = initialJob();
  }
}
