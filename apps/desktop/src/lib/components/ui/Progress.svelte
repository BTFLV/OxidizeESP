<script lang="ts">
  import { cn } from '$lib/utils/cn';

  let {
    value = null,
    tone = 'accent',
    label,
    class: className,
  }: {
    /** Fraction from 0 to 1, or null for an operation without measurable progress. */
    value?: number | null;
    tone?: 'accent' | 'success' | 'danger' | 'warning';
    label: string;
    class?: string;
  } = $props();

  const colors = { accent: 'bg-accent', success: 'bg-success', danger: 'bg-danger', warning: 'bg-warning' };
  const percent = $derived(value === null ? null : Math.round(Math.max(0, Math.min(1, value)) * 1000) / 10);
</script>

<div
  role="progressbar"
  aria-label={label}
  aria-valuemin={0}
  aria-valuemax={100}
  aria-valuenow={percent ?? undefined}
  class={cn('relative h-1.5 w-full overflow-hidden rounded-full bg-surface-3', className)}
>
  {#if percent === null}
    <div class={cn('absolute inset-y-0 w-1/3 animate-[indeterminate_1.4s_ease-in-out_infinite] rounded-full', colors[tone])}></div>
  {:else}
    <div class={cn('h-full rounded-full transition-[width] duration-200', colors[tone])} style:width="{percent}%"></div>
  {/if}
</div>

<style>
  @keyframes -global-indeterminate {
    0% {
      left: -35%;
    }
    100% {
      left: 100%;
    }
  }
</style>
