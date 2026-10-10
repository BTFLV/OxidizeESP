<script lang="ts">
  import { CircleAlert, CircleCheck, Info, TriangleAlert, X } from '$lib/icons';
  import { toasts } from '$lib/stores/toasts.svelte';
  import { cn } from '$lib/utils/cn';

  const icons = { info: Info, success: CircleCheck, warning: TriangleAlert, danger: CircleAlert };
  const colors = { info: 'text-info', success: 'text-success', warning: 'text-warning', danger: 'text-danger' };
</script>

<div class="pointer-events-none fixed right-4 bottom-10 z-[60] flex w-80 flex-col gap-2" aria-live="polite">
  {#each toasts.items as toast (toast.id)}
    {@const Icon = icons[toast.tone]}
    <div class="pointer-events-auto flex gap-2.5 rounded-lg border border-border-strong bg-surface-2 p-3 shadow-popover" role="status">
      <Icon class={cn('mt-px size-4 shrink-0', colors[toast.tone])} aria-hidden="true" />
      <div class="min-w-0 flex-1">
        <p class="text-[13px] font-medium break-words text-fg">{toast.title}</p>
        {#if toast.description}<p class="mt-0.5 text-xs break-words text-fg-muted">{toast.description}</p>{/if}
      </div>
      <button
        type="button"
        class="-mt-0.5 -mr-1 inline-flex size-6 shrink-0 items-center justify-center rounded text-fg-subtle outline-none hover:bg-surface-3 hover:text-fg focus-visible:ring-2 focus-visible:ring-focus/60"
        aria-label="Dismiss"
        onclick={() => toasts.dismiss(toast.id)}
      >
        <X class="size-3.5" />
      </button>
    </div>
  {/each}
</div>
