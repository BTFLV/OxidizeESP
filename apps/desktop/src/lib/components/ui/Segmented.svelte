<script lang="ts" generics="T extends string">
  import { RadioGroup } from 'bits-ui';
  import type { Component } from 'svelte';
  import { cn } from '$lib/utils/cn';

  let {
    options,
    value = $bindable(),
    label,
    class: className,
    onValueChange,
  }: {
    options: { value: T; label: string; icon?: Component; disabled?: boolean }[];
    value: T;
    label: string;
    class?: string;
    onValueChange?: (value: T) => void;
  } = $props();
</script>

<RadioGroup.Root
  aria-label={label}
  orientation="horizontal"
  bind:value={() => value, (v) => (value = v as T)}
  onValueChange={(v) => onValueChange?.(v as T)}
  class={cn('inline-flex h-8 items-center gap-0.5 rounded-lg border border-border bg-surface-2 p-0.5', className)}
>
  {#each options as option (option.value)}
    <RadioGroup.Item
      value={option.value}
      disabled={option.disabled ?? false}
      class="inline-flex h-full flex-1 items-center justify-center gap-1.5 rounded-md px-3 text-xs font-medium whitespace-nowrap text-fg-muted transition-colors outline-none hover:text-fg focus-visible:ring-2 focus-visible:ring-focus/60 disabled:opacity-40 data-[state=checked]:bg-surface data-[state=checked]:text-fg data-[state=checked]:shadow-sm [&_svg]:size-3.5"
    >
      {#if option.icon}<option.icon aria-hidden="true" />{/if}
      {option.label}
    </RadioGroup.Item>
  {/each}
</RadioGroup.Root>
