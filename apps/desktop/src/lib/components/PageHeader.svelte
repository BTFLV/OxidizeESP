<script lang="ts">
  import type { Snippet } from 'svelte';

  let { title, description, actions }: { title: string; description?: string; actions?: Snippet } = $props();
</script>

<div class="flex items-end justify-between gap-4">
  <div class="min-w-0">
    <h1 class="text-lg font-semibold tracking-tight text-fg">{title}</h1>
    {#if description}<p class="mt-0.5 text-[13px] text-fg-muted">{description}</p>{/if}
  </div>
  {#if actions}<div class="flex shrink-0 items-center gap-2">{@render actions()}</div>{/if}
</div>
