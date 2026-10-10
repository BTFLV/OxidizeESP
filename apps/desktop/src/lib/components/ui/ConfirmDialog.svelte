<!--
  Confirmation for consequential actions. Cancel receives focus by default.
  With `requireText`, the user must type the given text before the action
  can be confirmed.
-->
<script lang="ts">
  import { AlertDialog } from 'bits-ui';
  import type { Snippet } from 'svelte';
  import { TriangleAlert } from '$lib/icons';
  import { cn } from '$lib/utils/cn';
  import Button from './Button.svelte';
  import Input from './Input.svelte';

  let {
    open = $bindable(false),
    title,
    description,
    confirmLabel = 'Confirm',
    tone = 'danger',
    requireText = null,
    details,
    onConfirm,
  }: {
    open?: boolean;
    title: string;
    description: string;
    confirmLabel?: string;
    tone?: 'danger' | 'warning';
    requireText?: string | null;
    details?: Snippet;
    onConfirm: () => void;
  } = $props();

  let typed = $state('');
  let cancelButton = $state<HTMLButtonElement | null>(null);
  const canConfirm = $derived(!requireText || typed.trim() === requireText);

  $effect(() => {
    if (!open) typed = '';
  });

  function confirm() {
    if (!canConfirm) return;
    open = false;
    onConfirm();
  }
</script>

<AlertDialog.Root bind:open>
  <AlertDialog.Portal>
    <AlertDialog.Overlay class="fixed inset-0 z-50 bg-overlay backdrop-blur-[1px]" />
    <AlertDialog.Content
      onOpenAutoFocus={(e) => {
        e.preventDefault();
        cancelButton?.focus();
      }}
      class="fixed top-1/2 left-1/2 z-50 flex w-[min(30rem,calc(100vw-2rem))] -translate-x-1/2 -translate-y-1/2 flex-col gap-4 rounded-xl border border-border-strong bg-surface p-5 shadow-popover outline-none"
    >
      <div class="flex gap-3">
        <div
          class={cn(
            'flex size-9 shrink-0 items-center justify-center rounded-lg',
            tone === 'danger' ? 'bg-danger-soft text-danger' : 'bg-warning-soft text-warning',
          )}
        >
          <TriangleAlert class="size-[18px]" aria-hidden="true" />
        </div>
        <div class="flex min-w-0 flex-col gap-1">
          <AlertDialog.Title class="text-[15px] font-semibold text-fg">{title}</AlertDialog.Title>
          <AlertDialog.Description class="text-[13px] text-fg-muted">{description}</AlertDialog.Description>
        </div>
      </div>

      {#if details}
        <div class="rounded-lg border border-border bg-surface-2 px-3 py-2.5">{@render details()}</div>
      {/if}

      {#if requireText}
        <div class="flex flex-col gap-1.5">
          <label for="confirm-text" class="text-xs text-fg-muted">
            Type <span class="font-mono font-semibold text-fg">{requireText}</span> to confirm
          </label>
          <Input
            id="confirm-text"
            bind:value={typed}
            mono
            onkeydown={(e: KeyboardEvent) => {
              if (e.key === 'Enter') confirm();
            }}
          />
        </div>
      {/if}

      <div class="flex justify-end gap-2">
        <AlertDialog.Cancel>
          {#snippet child({ props })}
            <Button {...props} bind:ref={cancelButton}>Cancel</Button>
          {/snippet}
        </AlertDialog.Cancel>
        <Button variant={tone === 'danger' ? 'danger' : 'primary'} disabled={!canConfirm} onclick={confirm}>
          {confirmLabel}
        </Button>
      </div>
    </AlertDialog.Content>
  </AlertDialog.Portal>
</AlertDialog.Root>
