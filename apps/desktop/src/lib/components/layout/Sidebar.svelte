<script lang="ts">
  import { app } from '$lib/stores/app.svelte';
  import { monitor } from '$lib/stores/monitor.svelte';
  import { nav } from '$lib/stores/nav.svelte';
  import { cn } from '$lib/utils/cn';
  import { VIEWS } from '$lib/navigation';
</script>

<nav
  aria-label="Main"
  class="flex h-full w-[200px] shrink-0 flex-col border-r border-border bg-surface"
>
  <div
    data-tauri-drag-region
    class={cn('flex h-12 shrink-0 items-center gap-2 px-4', app.isMac && 'h-[52px] items-end pb-2.5 pl-[84px]')}
  >
    <img src="/icon.svg" alt="" class="pointer-events-none size-5" />
    <span class="pointer-events-none text-[13px] font-semibold tracking-tight text-fg">OxidizeESP</span>
  </div>
  <ul class="flex flex-1 flex-col gap-0.5 px-2 py-2">
    {#each VIEWS as view, index (view.id)}
      {@const active = nav.view === view.id}
      <li>
        <button
          type="button"
          onclick={() => nav.go(view.id)}
          aria-current={active ? 'page' : undefined}
          class={cn(
            'group flex h-8 w-full items-center gap-2.5 rounded-md px-2.5 text-[13px] transition-colors outline-none focus-visible:ring-2 focus-visible:ring-focus/60',
            active ? 'bg-surface-3 font-medium text-fg' : 'text-fg-muted hover:bg-surface-2 hover:text-fg',
          )}
        >
          <view.icon class={cn('size-4 shrink-0', active ? 'text-accent' : 'text-fg-subtle group-hover:text-fg-muted')} aria-hidden="true" />
          <span class="flex-1 truncate text-left">{view.label}</span>
          {#if view.id === 'monitor' && monitor.status === 'running'}
            <span class="size-1.5 rounded-full bg-success" aria-label="Monitor running"></span>
          {:else if index < 9}
            <kbd class="hidden font-sans text-2xs text-fg-subtle group-hover:inline">{app.modKey}{index + 1}</kbd>
          {/if}
        </button>
      </li>
    {/each}
  </ul>
</nav>
