<script lang="ts">
  import { Checkbox } from 'bits-ui';
  import { Check } from '$lib/icons';
  import { cn } from '$lib/utils/cn';

  let {
    checked = $bindable(false),
    label,
    description,
    disabled = false,
    hideLabel = false,
    class: className,
    onCheckedChange,
  }: {
    checked?: boolean;
    label: string;
    description?: string;
    disabled?: boolean;
    /** Keep the label for assistive technology only. */
    hideLabel?: boolean;
    class?: string;
    onCheckedChange?: (checked: boolean) => void;
  } = $props();

  const id = `checkbox-${Math.random().toString(36).slice(2, 9)}`;
</script>

<div class={cn('flex items-start gap-2.5', className)}>
  <Checkbox.Root
    {id}
    bind:checked
    {disabled}
    onCheckedChange={(v) => onCheckedChange?.(v)}
    class="mt-0.5 inline-flex size-4 shrink-0 items-center justify-center rounded border border-border-strong bg-surface transition-colors outline-none focus-visible:ring-2 focus-visible:ring-focus/60 disabled:opacity-50 data-[state=checked]:border-accent data-[state=checked]:bg-accent"
  >
    {#snippet children({ checked: isChecked })}
      {#if isChecked}<Check class="size-3 text-accent-fg" strokeWidth={3} aria-hidden="true" />{/if}
    {/snippet}
  </Checkbox.Root>
  <label for={id} class={cn('flex min-w-0 flex-col', disabled && 'opacity-50', hideLabel && 'sr-only')}>
    <span class="text-[13px] leading-5 text-fg">{label}</span>
    {#if description}<span class="text-xs text-fg-subtle">{description}</span>{/if}
  </label>
</div>
