<script lang="ts">
  import { Switch } from 'bits-ui';
  import { cn } from '$lib/utils/cn';

  let {
    checked = $bindable(false),
    label,
    description,
    disabled = false,
    id,
    class: className,
    onCheckedChange,
  }: {
    checked?: boolean;
    label: string;
    description?: string;
    disabled?: boolean;
    id?: string;
    class?: string;
    onCheckedChange?: (checked: boolean) => void;
  } = $props();

  const generatedId = `switch-${Math.random().toString(36).slice(2, 9)}`;
  const switchId = $derived(id ?? generatedId);
</script>

<div class={cn('flex items-start justify-between gap-4', className)}>
  <label for={switchId} class={cn('flex min-w-0 flex-col', disabled && 'opacity-50')}>
    <span class="text-[13px] text-fg">{label}</span>
    {#if description}<span class="text-xs text-fg-subtle">{description}</span>{/if}
  </label>
  <Switch.Root
    id={switchId}
    bind:checked
    {disabled}
    onCheckedChange={(v) => onCheckedChange?.(v)}
    class="peer mt-0.5 inline-flex h-[18px] w-8 shrink-0 items-center rounded-full border border-transparent bg-surface-3 transition-colors outline-none focus-visible:ring-2 focus-visible:ring-focus/60 disabled:opacity-50 data-[state=checked]:bg-accent"
  >
    <Switch.Thumb
      class="pointer-events-none block size-3.5 translate-x-0.5 rounded-full bg-fg shadow-sm transition-transform data-[state=checked]:translate-x-[15px] data-[state=checked]:bg-accent-fg"
    />
  </Switch.Root>
</div>
