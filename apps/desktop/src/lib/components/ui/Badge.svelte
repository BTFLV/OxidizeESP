<script lang="ts" module>
  export type BadgeTone = 'neutral' | 'accent' | 'success' | 'warning' | 'danger' | 'info';
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';
  import { cn } from '$lib/utils/cn';

  let {
    tone = 'neutral',
    dot = false,
    class: className,
    children,
  }: { tone?: BadgeTone; dot?: boolean; class?: string; children: Snippet } = $props();

  const tones: Record<BadgeTone, string> = {
    neutral: 'bg-surface-3 text-fg-muted',
    accent: 'bg-accent-soft text-accent',
    success: 'bg-success-soft text-success',
    warning: 'bg-warning-soft text-warning',
    danger: 'bg-danger-soft text-danger',
    info: 'bg-info-soft text-info',
  };
  const dots: Record<BadgeTone, string> = {
    neutral: 'bg-fg-subtle',
    accent: 'bg-accent',
    success: 'bg-success',
    warning: 'bg-warning',
    danger: 'bg-danger',
    info: 'bg-info',
  };
</script>

<span
  class={cn(
    'inline-flex h-5 shrink-0 items-center gap-1.5 rounded px-1.5 text-2xs font-medium whitespace-nowrap',
    tones[tone],
    className,
  )}
>
  {#if dot}<span class={cn('size-1.5 rounded-full', dots[tone])} aria-hidden="true"></span>{/if}
  {@render children()}
</span>
