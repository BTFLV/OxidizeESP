<script lang="ts" module>
  export type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger' | 'outline';
  export type ButtonSize = 'sm' | 'md' | 'icon' | 'icon-sm';
</script>

<script lang="ts">
  import type { HTMLButtonAttributes } from 'svelte/elements';
  import { cn } from '$lib/utils/cn';
  import Spinner from './Spinner.svelte';

  let {
    variant = 'secondary',
    size = 'md',
    loading = false,
    class: className,
    children,
    disabled,
    type = 'button',
    ref = $bindable(null),
    ...rest
  }: HTMLButtonAttributes & {
    variant?: ButtonVariant;
    size?: ButtonSize;
    loading?: boolean;
    ref?: HTMLButtonElement | null;
  } = $props();

  const variants: Record<ButtonVariant, string> = {
    primary: 'bg-accent text-accent-fg hover:bg-accent-hover shadow-sm',
    secondary: 'bg-surface-2 text-fg border border-border hover:bg-surface-3 hover:border-border-strong',
    ghost: 'text-fg-muted hover:bg-surface-2 hover:text-fg',
    danger: 'bg-danger text-danger-fg hover:bg-danger-hover shadow-sm',
    outline: 'border border-border-strong text-fg hover:bg-surface-2',
  };
  const sizes: Record<ButtonSize, string> = {
    sm: 'h-7 px-2.5 gap-1.5 text-xs [&_svg]:size-3.5',
    md: 'h-8 px-3 gap-2 text-[13px] [&_svg]:size-4',
    icon: 'size-8 [&_svg]:size-4',
    'icon-sm': 'size-7 [&_svg]:size-3.5',
  };
</script>

<button
  bind:this={ref}
  {type}
  {...rest}
  disabled={disabled || loading}
  class={cn(
    'inline-flex shrink-0 items-center justify-center rounded-md font-medium whitespace-nowrap transition-colors duration-100 outline-none select-none',
    'focus-visible:ring-2 focus-visible:ring-focus/60 focus-visible:outline-none disabled:pointer-events-none disabled:opacity-45',
    variants[variant],
    sizes[size],
    className,
  )}
>
  {#if loading}<Spinner class="size-3.5" />{/if}
  {@render children?.()}
</button>
