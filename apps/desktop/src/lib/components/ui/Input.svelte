<script lang="ts">
  import type { HTMLInputAttributes } from 'svelte/elements';
  import { cn } from '$lib/utils/cn';

  let {
    value = $bindable(''),
    invalid = false,
    mono = false,
    class: className,
    ref = $bindable(null),
    ...rest
  }: Omit<HTMLInputAttributes, 'value'> & {
    value?: string;
    invalid?: boolean;
    mono?: boolean;
    ref?: HTMLInputElement | null;
  } = $props();
</script>

<input
  bind:this={ref}
  bind:value
  spellcheck="false"
  autocomplete="off"
  autocapitalize="off"
  aria-invalid={invalid || undefined}
  {...rest}
  class={cn(
    'h-8 w-full min-w-0 rounded-md border bg-surface px-2.5 text-[13px] text-fg transition-colors outline-none placeholder:text-fg-subtle',
    'focus:border-accent focus:ring-2 focus:ring-accent/20 disabled:cursor-not-allowed disabled:opacity-50',
    invalid ? 'border-danger focus:border-danger focus:ring-danger/20' : 'border-border hover:border-border-strong',
    mono && 'font-mono text-xs tabular',
    className,
  )}
/>
