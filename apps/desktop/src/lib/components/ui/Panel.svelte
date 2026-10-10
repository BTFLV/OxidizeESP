<script lang="ts">
  import type { Snippet } from 'svelte';
  import { cn } from '$lib/utils/cn';

  let {
    title,
    description,
    class: className,
    bodyClass,
    actions,
    children,
  }: {
    title?: string;
    description?: string;
    class?: string;
    bodyClass?: string;
    actions?: Snippet;
    children: Snippet;
  } = $props();
</script>

<section class={cn('flex min-w-0 flex-col rounded-lg border border-border bg-surface', className)}>
  {#if title || actions}
    <header class="flex min-h-11 items-center justify-between gap-3 border-b border-border px-4 py-2">
      <div class="min-w-0">
        {#if title}<h2 class="truncate text-[13px] font-semibold text-fg">{title}</h2>{/if}
        {#if description}<p class="truncate text-xs text-fg-subtle">{description}</p>{/if}
      </div>
      {#if actions}<div class="flex shrink-0 items-center gap-1.5">{@render actions()}</div>{/if}
    </header>
  {/if}
  <div class={cn('p-4', bodyClass)}>
    {@render children()}
  </div>
</section>
