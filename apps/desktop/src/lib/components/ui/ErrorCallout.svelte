<script lang="ts">
  import type { Snippet } from 'svelte';
  import { type AppError, errorTitle } from '$lib/utils/errors';
  import Callout from './Callout.svelte';

  let {
    error,
    title,
    class: className,
    actions,
  }: { error: AppError; title?: string; class?: string; actions?: Snippet } = $props();

  const tone = $derived(error.kind === 'cancelled' ? 'warning' : 'danger');
</script>

<Callout {tone} title={title ?? errorTitle(error)} class={className} {actions}>
  <p class="selectable">{error.message}</p>
  {#if error.hint}<p class="mt-1 text-fg-muted">{error.hint}</p>{/if}
  {#if error.detail}
    <details class="mt-1.5 text-xs">
      <summary class="cursor-default text-fg-subtle hover:text-fg-muted">Technical details</summary>
      <pre class="mt-1.5 max-h-40 overflow-auto rounded border border-border bg-surface px-2 py-1.5 font-mono text-2xs whitespace-pre-wrap text-fg-muted">{error.detail}</pre>
    </details>
  {/if}
</Callout>
