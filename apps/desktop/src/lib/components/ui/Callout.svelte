<script lang="ts" module>
  export type CalloutTone = 'info' | 'success' | 'warning' | 'danger' | 'neutral';
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';
  import { CircleAlert, CircleCheck, Info, TriangleAlert } from '$lib/icons';
  import { cn } from '$lib/utils/cn';

  let {
    tone = 'info',
    title,
    class: className,
    children,
    actions,
  }: {
    tone?: CalloutTone;
    title?: string;
    class?: string;
    children?: Snippet;
    actions?: Snippet;
  } = $props();

  const styles: Record<CalloutTone, string> = {
    info: 'border-info/25 bg-info-soft',
    success: 'border-success/25 bg-success-soft',
    warning: 'border-warning/30 bg-warning-soft',
    danger: 'border-danger/30 bg-danger-soft',
    neutral: 'border-border bg-surface-2',
  };
  const iconColor: Record<CalloutTone, string> = {
    info: 'text-info',
    success: 'text-success',
    warning: 'text-warning',
    danger: 'text-danger',
    neutral: 'text-fg-muted',
  };
  const Icon = $derived(
    tone === 'success' ? CircleCheck : tone === 'warning' ? TriangleAlert : tone === 'danger' ? CircleAlert : Info,
  );
</script>

<div
  class={cn('flex gap-2.5 rounded-lg border px-3 py-2.5', styles[tone], className)}
  role={tone === 'danger' || tone === 'warning' ? 'alert' : 'status'}
>
  <Icon class={cn('mt-px size-4 shrink-0', iconColor[tone])} aria-hidden="true" />
  <div class="flex min-w-0 flex-1 flex-col gap-1 text-[13px]">
    {#if title}<p class="font-medium text-fg">{title}</p>{/if}
    {#if children}<div class="text-fg-muted [&_strong]:text-fg">{@render children()}</div>{/if}
    {#if actions}<div class="mt-1 flex flex-wrap gap-2">{@render actions()}</div>{/if}
  </div>
</div>
