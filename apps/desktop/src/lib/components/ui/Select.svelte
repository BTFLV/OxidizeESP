<script lang="ts" module>
  export interface SelectItem {
    value: string;
    label: string;
    hint?: string;
    disabled?: boolean;
  }
</script>

<script lang="ts">
  import { Select } from 'bits-ui';
  import { Check, ChevronDown } from '$lib/icons';
  import { cn } from '$lib/utils/cn';

  let {
    items,
    value = $bindable(''),
    placeholder = 'Select…',
    disabled = false,
    id,
    class: className,
    contentClass,
    mono = false,
    'aria-label': ariaLabel,
    onValueChange,
  }: {
    items: SelectItem[];
    value?: string;
    placeholder?: string;
    disabled?: boolean;
    id?: string;
    class?: string;
    contentClass?: string;
    mono?: boolean;
    'aria-label'?: string;
    onValueChange?: (value: string) => void;
  } = $props();

  const selected = $derived(items.find((i) => i.value === value));
</script>

<Select.Root
  type="single"
  bind:value
  {disabled}
  items={items.map((i) => ({ value: i.value, label: i.label, disabled: i.disabled ?? false }))}
  onValueChange={(v) => onValueChange?.(v)}
>
  <Select.Trigger
    {id}
    aria-label={ariaLabel}
    class={cn(
      'inline-flex h-8 w-full min-w-0 items-center justify-between gap-2 rounded-md border border-border bg-surface px-2.5 text-left text-[13px] text-fg transition-colors outline-none',
      'hover:border-border-strong focus-visible:border-accent focus-visible:ring-2 focus-visible:ring-accent/20 disabled:cursor-not-allowed disabled:opacity-50 data-[state=open]:border-accent',
      mono && 'font-mono text-xs',
      className,
    )}
  >
    <span class={cn('truncate', !selected && 'text-fg-subtle')}>{selected?.label ?? placeholder}</span>
    <ChevronDown class="size-3.5 shrink-0 text-fg-subtle" aria-hidden="true" />
  </Select.Trigger>
  <Select.Portal>
    <Select.Content
      sideOffset={4}
      class={cn(
        'z-50 max-h-80 min-w-(--bits-select-anchor-width) overflow-hidden rounded-lg border border-border-strong bg-surface p-1 shadow-popover outline-none',
        contentClass,
      )}
    >
      <Select.Viewport class="max-h-72">
        {#each items as item (item.value)}
          <Select.Item
            value={item.value}
            label={item.label}
            disabled={item.disabled ?? false}
            class="flex cursor-default items-start gap-2 rounded-md px-2 py-1.5 text-[13px] text-fg outline-none select-none data-disabled:opacity-40 data-highlighted:bg-surface-3"
          >
            {#snippet children({ selected: isSelected })}
              <Check class={cn('mt-0.5 size-3.5 shrink-0 text-accent', !isSelected && 'invisible')} aria-hidden="true" />
              <span class="flex min-w-0 flex-col">
                <span class={cn('truncate', mono && 'font-mono text-xs')}>{item.label}</span>
                {#if item.hint}<span class="text-xs text-fg-subtle">{item.hint}</span>{/if}
              </span>
            {/snippet}
          </Select.Item>
        {/each}
      </Select.Viewport>
    </Select.Content>
  </Select.Portal>
</Select.Root>
