<!--
  Button that explains why it is unavailable. With `disabledReason` set, the
  button is marked aria-disabled, ignores activation, and shows the reason as
  a tooltip and accessible description. It stays focusable so keyboard users
  can discover the reason.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { cn } from '$lib/utils/cn';
  import Button, { type ButtonSize, type ButtonVariant } from './Button.svelte';
  import Tooltip from './Tooltip.svelte';

  let {
    disabledReason = null,
    variant = 'secondary',
    size = 'md',
    loading = false,
    class: className,
    onclick,
    children: content,
    title,
    'aria-label': ariaLabel,
  }: {
    disabledReason?: string | null;
    variant?: ButtonVariant;
    size?: ButtonSize;
    loading?: boolean;
    class?: string;
    onclick?: (event: MouseEvent) => void;
    children: Snippet;
    title?: string;
    'aria-label'?: string;
  } = $props();

  const reasonId = `reason-${Math.random().toString(36).slice(2, 9)}`;
  const unavailable = $derived(!!disabledReason);

  function handleClick(event: MouseEvent) {
    if (unavailable || loading) {
      event.preventDefault();
      return;
    }
    onclick?.(event);
  }
</script>

<Tooltip text={disabledReason ?? title}>
  {#snippet children(props)}
    <Button
      {...props}
      {variant}
      {size}
      {loading}
      class={cn(unavailable && 'cursor-not-allowed opacity-45 hover:brightness-100', className)}
      aria-disabled={unavailable ? 'true' : undefined}
      aria-describedby={unavailable ? reasonId : undefined}
      aria-label={ariaLabel}
      onclick={handleClick}
    >
      {@render content()}
    </Button>
  {/snippet}
</Tooltip>
{#if disabledReason}<span id={reasonId} class="sr-only">{disabledReason}</span>{/if}
