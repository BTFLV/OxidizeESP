<script lang="ts">
  import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
  import { File, FolderOpen, X } from '$lib/icons';
  import { byExtension, dropZones } from '$lib/stores/dropzones.svelte';
  import { cn } from '$lib/utils/cn';
  import { basename, dirname, joinPath } from '$lib/utils/format';
  import Button from './Button.svelte';

  let {
    path = $bindable(null),
    mode = 'open',
    filters = [],
    defaultPath = null,
    suggestedName = null,
    placeholder = 'No file selected',
    dropHint = 'Drop a file here or browse',
    disabled = false,
    allowClear = true,
    compact = false,
    id,
    'aria-label': ariaLabel,
    onSelect,
  }: {
    path?: string | null;
    mode?: 'open' | 'save';
    filters?: { name: string; extensions: string[] }[];
    defaultPath?: string | null;
    suggestedName?: string | null;
    placeholder?: string;
    dropHint?: string;
    disabled?: boolean;
    allowClear?: boolean;
    /** Single-line variant for toolbars. */
    compact?: boolean;
    id?: string;
    'aria-label'?: string;
    onSelect?: (path: string | null) => void;
  } = $props();

  let element = $state<HTMLElement | null>(null);
  const extensions = $derived(filters.flatMap((f) => f.extensions).filter((e) => e !== '*'));
  const hovered = $derived(element !== null && dropZones.hovered === element);

  $effect(() => {
    if (!element || mode !== 'open' || disabled) return;
    return dropZones.register(element, {
      accept: byExtension(extensions.length ? extensions : null),
      onDrop: (p) => select(p),
    });
  });

  function select(next: string | null) {
    path = next;
    onSelect?.(next);
  }

  async function browse() {
    const start = path ? (mode === 'save' ? path : dirname(path)) : defaultPath;
    const savePath = start && suggestedName && mode === 'save' && !path ? joinPath(start, suggestedName) : start;
    const result =
      mode === 'open'
        ? await openDialog({ multiple: false, directory: false, filters, defaultPath: start ?? undefined })
        : await saveDialog({ filters, defaultPath: savePath ?? suggestedName ?? undefined });
    if (typeof result === 'string') select(result);
  }
</script>

<div
  bind:this={element}
  class={cn(
    'flex min-w-0 items-center rounded-md border bg-surface transition-colors',
    compact ? 'h-8 gap-2 pr-0.5 pl-2' : 'min-h-10 gap-2.5 px-2.5 py-1.5',
    path ? 'border-border' : 'border-dashed border-border-strong',
    hovered && 'border-accent bg-accent-soft',
    disabled && 'opacity-50',
  )}
>
  <File class={cn('shrink-0', compact ? 'size-3.5' : 'size-4', path ? 'text-fg-muted' : 'text-fg-subtle')} aria-hidden="true" />
  <div class="flex min-w-0 flex-1 flex-col">
    {#if path}
      <span class={cn('truncate text-fg selectable', compact ? 'text-xs' : 'text-[13px]')} title={path}>{basename(path)}</span>
      {#if !compact}<span class="truncate text-2xs text-fg-subtle" title={path}>{dirname(path)}</span>{/if}
    {:else}
      <span class={cn('truncate text-fg-subtle', compact ? 'text-xs' : 'text-[13px]')}>{mode === 'open' ? dropHint : placeholder}</span>
    {/if}
  </div>
  {#if path && allowClear}
    <Button variant="ghost" size="icon-sm" onclick={() => select(null)} {disabled} aria-label="Clear file selection">
      <X />
    </Button>
  {/if}
  <Button
    size={compact ? 'icon-sm' : 'sm'}
    variant={compact ? 'ghost' : 'secondary'}
    onclick={browse}
    {disabled}
    {id}
    aria-label={ariaLabel ?? (mode === 'open' ? 'Browse for a file' : 'Choose where to save')}
  >
    <FolderOpen />
    {#if !compact}{mode === 'open' ? 'Browse…' : 'Choose…'}{/if}
  </Button>
</div>
