<!--
  Progress, log and outcome of an operation run through a JobRunner.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { ChevronDown, ChevronRight, CircleCheck, CircleX, Square, TriangleAlert } from '$lib/icons';
  import type { JobRunner } from '$lib/stores/job.svelte';
  import { cn } from '$lib/utils/cn';
  import { formatDuration, formatPercent, formatRate, formatSize, formatTime } from '$lib/utils/format';
  import ActionButton from './ui/ActionButton.svelte';
  import ErrorCallout from './ui/ErrorCallout.svelte';
  import Progress from './ui/Progress.svelte';
  import Spinner from './ui/Spinner.svelte';

  let {
    runner,
    success,
    class: className,
  }: { runner: JobRunner; success?: Snippet; class?: string } = $props();

  const s = $derived(runner.state);
  let showLog = $state(false);
  let now = $state(Date.now());

  $effect(() => {
    if (s.phase !== 'running') return;
    const timer = setInterval(() => (now = Date.now()), 500);
    return () => clearInterval(timer);
  });

  const elapsed = $derived(s.startedAt ? (s.finishedAt ?? now) - s.startedAt : 0);
  const eta = $derived.by(() => {
    const p = s.progress;
    if (!p || p.unit !== 'bytes' || !s.bytesPerSecond || p.current >= p.total) return null;
    return ((p.total - p.current) / s.bytesPerSecond) * 1000;
  });
  const progressText = $derived.by(() => {
    const p = s.progress;
    if (!p) return '';
    if (p.unit === 'bytes') return `${formatSize(p.current)} of ${formatSize(p.total)}`;
    return p.detail ?? '';
  });
</script>

{#if s.phase !== 'idle'}
  <div class={cn('flex flex-col gap-3 rounded-lg border border-border bg-surface p-4', className)} aria-live="polite">
    {#if s.phase === 'running'}
      <div class="flex items-start justify-between gap-3">
        <div class="flex min-w-0 items-start gap-2.5">
          <Spinner class="mt-0.5 shrink-0 text-accent" />
          <div class="min-w-0">
            <p class="text-[13px] font-medium text-fg">{runner.label}</p>
            <p class="truncate text-xs text-fg-muted">{s.stageMessage || 'Starting…'}</p>
          </div>
        </div>
        <ActionButton
          size="sm"
          variant="outline"
          disabledReason={!s.cancellable
            ? 'This operation cannot be interrupted safely. Interrupting a write could leave the flash in an inconsistent state.'
            : s.cancelRequested
              ? 'Cancellation requested'
              : null}
          onclick={() => runner.cancel()}
        >
          <Square />
          {s.cancelRequested ? 'Cancelling…' : 'Cancel'}
        </ActionButton>
      </div>
      <Progress value={s.progress ? s.progress.overall : null} label={runner.label} />
      <div class="flex items-center justify-between gap-3 text-xs text-fg-subtle tabular">
        <span class="truncate">
          {#if s.progress}{formatPercent(s.progress.overall)} · {progressText}{:else}Waiting for the device…{/if}
        </span>
        <span class="shrink-0">
          {#if s.bytesPerSecond}{formatRate(s.bytesPerSecond)} ·{/if}
          {#if eta !== null}{formatDuration(eta)} left ·{/if}
          {formatDuration(elapsed)}
        </span>
      </div>
    {:else if s.phase === 'completed'}
      <div class="flex items-start gap-2.5">
        <CircleCheck class="mt-px size-4 shrink-0 text-success" aria-hidden="true" />
        <div class="flex min-w-0 flex-1 flex-col gap-2">
          <div>
            <p class="text-[13px] font-medium text-fg">{s.message ?? 'Completed'}</p>
            <p class="text-xs text-fg-subtle">Finished in {formatDuration(elapsed)}</p>
          </div>
          {@render success?.()}
        </div>
      </div>
    {:else if s.phase === 'cancelled'}
      <div class="flex items-start gap-2.5">
        <TriangleAlert class="mt-px size-4 shrink-0 text-warning" aria-hidden="true" />
        <div class="min-w-0">
          <p class="text-[13px] font-medium text-fg">Cancelled</p>
          <p class="text-xs text-fg-muted">{s.message}</p>
        </div>
      </div>
    {:else if s.phase === 'failed' && s.error}
      <ErrorCallout error={s.error} title={`${runner.label} failed`} />
    {/if}

    {#if s.flashMayBeInconsistent && s.phase !== 'completed'}
      <div class="flex items-start gap-2 rounded-md bg-warning-soft px-3 py-2 text-xs text-warning">
        <CircleX class="mt-px size-3.5 shrink-0" aria-hidden="true" />
        The operation stopped after flash was modified. The device may not boot until the operation is repeated
        successfully.
      </div>
    {/if}

    {#if s.log.length > 0}
      <div>
        <button
          type="button"
          class="inline-flex items-center gap-1 rounded text-xs text-fg-subtle outline-none hover:text-fg-muted focus-visible:ring-2 focus-visible:ring-focus/60"
          aria-expanded={showLog}
          onclick={() => (showLog = !showLog)}
        >
          {#if showLog}<ChevronDown class="size-3.5" />{:else}<ChevronRight class="size-3.5" />{/if}
          Operation log ({s.log.length})
        </button>
        {#if showLog}
          <ol class="mt-2 max-h-48 overflow-auto rounded-md border border-border bg-log-bg px-3 py-2 font-mono text-2xs leading-5">
            {#each s.log as entry, i (i)}
              <li
                class={cn(
                  'flex gap-3',
                  entry.level === 'error' && 'text-danger',
                  entry.level === 'warning' && 'text-warning',
                  entry.level === 'success' && 'text-success',
                  entry.level === 'info' && 'text-fg-muted',
                  entry.level === 'stage' && 'text-fg',
                )}
              >
                <span class="shrink-0 text-fg-subtle">{formatTime(entry.time, false)}</span>
                <span class="min-w-0 break-words">{entry.message}</span>
              </li>
            {/each}
          </ol>
        {/if}
      </div>
    {/if}
  </div>
{/if}
