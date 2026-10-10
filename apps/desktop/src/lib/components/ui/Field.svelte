<script lang="ts">
  import type { Snippet } from 'svelte';
  import { cn } from '$lib/utils/cn';

  let {
    label,
    id,
    hint,
    error,
    class: className,
    children,
    aside,
  }: {
    label: string;
    id?: string;
    hint?: string | null;
    error?: string | null;
    class?: string;
    children: Snippet;
    aside?: Snippet;
  } = $props();
</script>

<div class={cn('flex min-w-0 flex-col gap-1.5', className)}>
  <div class="flex items-center justify-between gap-2">
    <label for={id} class="text-xs font-medium text-fg-muted">{label}</label>
    {@render aside?.()}
  </div>
  {@render children()}
  {#if error}
    <p class="text-xs text-danger" role="alert">{error}</p>
  {:else if hint}
    <p class="text-xs text-fg-subtle">{hint}</p>
  {/if}
</div>
