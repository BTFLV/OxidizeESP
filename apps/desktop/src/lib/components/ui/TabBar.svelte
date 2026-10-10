<script lang="ts">
  import { Tabs } from 'bits-ui';
  import type { Component } from 'svelte';
  import { cn } from '$lib/utils/cn';

  let {
    items,
    class: className,
  }: {
    items: { value: string; label: string; icon?: Component; disabled?: boolean }[];
    class?: string;
  } = $props();
</script>

<Tabs.List class={cn('inline-flex h-8 items-center gap-0.5 rounded-lg border border-border bg-surface-2 p-0.5', className)}>
  {#each items as item (item.value)}
    <Tabs.Trigger
      value={item.value}
      disabled={item.disabled ?? false}
      class="inline-flex h-full items-center gap-1.5 rounded-md px-3 text-xs font-medium text-fg-muted transition-colors outline-none hover:text-fg focus-visible:ring-2 focus-visible:ring-focus/60 disabled:opacity-40 data-[state=active]:bg-surface data-[state=active]:text-fg data-[state=active]:shadow-sm [&_svg]:size-3.5"
    >
      {#if item.icon}<item.icon aria-hidden="true" />{/if}
      {item.label}
    </Tabs.Trigger>
  {/each}
</Tabs.List>
