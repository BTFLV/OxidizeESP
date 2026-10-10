<script lang="ts">
  import type { Component, Snippet } from 'svelte';
  import { cn } from '$lib/utils/cn';

  let {
    icon: Icon,
    title,
    description,
    class: className,
    children,
  }: { icon?: Component; title: string; description?: string; class?: string; children?: Snippet } = $props();
</script>

<div class={cn('flex flex-col items-center justify-center gap-2 px-6 py-10 text-center', className)}>
  {#if Icon}
    <div class="mb-1 flex size-10 items-center justify-center rounded-lg border border-border bg-surface-2 text-fg-subtle">
      <Icon class="size-5" aria-hidden="true" />
    </div>
  {/if}
  <p class="text-[13px] font-medium text-fg">{title}</p>
  {#if description}<p class="max-w-sm text-xs text-fg-subtle">{description}</p>{/if}
  {#if children}<div class="mt-2 flex flex-wrap justify-center gap-2">{@render children()}</div>{/if}
</div>
