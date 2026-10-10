<script lang="ts">
  import { Tooltip } from 'bits-ui';
  import type { Snippet } from 'svelte';

  let {
    text,
    side = 'top',
    children,
  }: {
    text: string | null | undefined;
    side?: 'top' | 'bottom' | 'left' | 'right';
    children: Snippet<[Record<string, unknown>]>;
  } = $props();
</script>

{#if text}
  <Tooltip.Root delayDuration={350}>
    <Tooltip.Trigger>
      {#snippet child({ props })}
        {@render children(props)}
      {/snippet}
    </Tooltip.Trigger>
    <Tooltip.Portal>
      <Tooltip.Content
        {side}
        sideOffset={6}
        class="z-50 max-w-72 rounded-md border border-border-strong bg-surface-3 px-2 py-1 text-xs text-fg shadow-popover"
      >
        {text}
      </Tooltip.Content>
    </Tooltip.Portal>
  </Tooltip.Root>
{:else}
  {@render children({})}
{/if}
