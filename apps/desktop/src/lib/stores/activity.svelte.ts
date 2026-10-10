import type { JobRunner } from './job.svelte';

/** Registry of operations currently running anywhere in the application. */
class ActivityRegistry {
  runners = $state<JobRunner[]>([]);

  register(runner: JobRunner): () => void {
    this.runners = [...this.runners, runner];
    return () => {
      this.runners = this.runners.filter((r) => r !== runner);
    };
  }

  get current(): JobRunner | null {
    return this.runners[this.runners.length - 1] ?? null;
  }

  isBusy(deviceId: string | null): boolean {
    return deviceId !== null && this.runners.some((r) => r.deviceId === deviceId);
  }
}

export const activity = new ActivityRegistry();
