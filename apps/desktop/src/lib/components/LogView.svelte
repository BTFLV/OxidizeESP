<!--
  Virtualized log display. Only the rows inside the viewport are rendered,
  so buffers with hundreds of thousands of lines stay responsive.
-->
<script lang="ts">
  import type { MonitorLine } from '$lib/bindings/MonitorLine';
  import { ArrowDownToLine } from '$lib/icons';
  import { parseAnsi } from '$lib/utils/ansi';
  import { cn } from '$lib/utils/cn';
  import { formatTime } from '$lib/utils/format';
  import { highlight } from '$lib/utils/highlight';

  let {
    rows,
    showTimestamps,
    autoscroll,
    search = '',
    activeRow = null,
  }: {
    rows: MonitorLine[];
    showTimestamps: boolean;
    autoscroll: boolean;
    search?: string;
    activeRow?: number | null;
  } = $props();

  const ROW_HEIGHT = 18;
  const OVERSCAN = 12;

  let viewport = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let height = $state(0);
  let following = $state(true);

  const start = $derived(Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN));
  const end = $derived(Math.min(rows.length, Math.ceil((scrollTop + height) / ROW_HEIGHT) + OVERSCAN));
  const visible = $derived(rows.slice(start, end));

  const ANSI_CLASSES = [
    'text-fg-subtle',
    'text-danger',
    'text-success',
    'text-warning',
    'text-info',
    'text-[#c678dd]',
    'text-[#3fb8c8]',
    'text-fg',
    'text-fg-muted',
    'text-danger',
    'text-success',
    'text-warning',
    'text-info',
    'text-[#d38be6]',
    'text-[#5fd0de]',
    'text-fg',
  ];

  function onScroll() {
    if (!viewport) return;
    scrollTop = viewport.scrollTop;
    const atBottom = viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight < ROW_HEIGHT * 2;
    following = atBottom;
  }

  function scrollToBottom() {
    if (!viewport) return;
    viewport.scrollTop = viewport.scrollHeight;
    following = true;
  }

  $effect(() => {
    if (!viewport) return;
    const observer = new ResizeObserver(() => (height = viewport?.clientHeight ?? 0));
    observer.observe(viewport);
    height = viewport.clientHeight;
    return () => observer.disconnect();
  });

  // Keep the newest line in view while following.
  $effect(() => {
    void rows.length;
    if (autoscroll && following && viewport) {
      queueMicrotask(() => {
        if (viewport) viewport.scrollTop = viewport.scrollHeight;
      });
    }
  });

  // Bring the active search match into view.
  $effect(() => {
    if (activeRow === null || !viewport) return;
    const top = activeRow * ROW_HEIGHT;
    if (top < viewport.scrollTop || top > viewport.scrollTop + viewport.clientHeight - ROW_HEIGHT) {
      viewport.scrollTop = Math.max(0, top - viewport.clientHeight / 2);
      following = false;
    }
  });
</script>

<div class="relative h-full min-h-0">
  <!-- The scrollable log must be focusable so it can be scrolled with the keyboard. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div
    bind:this={viewport}
    onscroll={onScroll}
    class="h-full overflow-auto bg-log-bg font-mono text-xs leading-[18px] selectable"
    role="log"
    aria-label="Serial output"
    tabindex="0"
  >
    <div class="relative min-w-max" style:height="{rows.length * ROW_HEIGHT}px">
      <div class="absolute inset-x-0 top-0" style:transform="translateY({start * ROW_HEIGHT}px)">
        {#each visible as line, i (line.seq)}
          {@const index = start + i}
          <div
            class={cn(
              'flex h-[18px] items-center gap-3 px-3 whitespace-pre',
              index === activeRow && 'bg-accent-soft',
              line.kind === 'system' && 'text-fg-subtle italic',
              line.kind === 'decoded' && 'text-info',
              line.kind === 'sent' && 'text-accent',
            )}
          >
            {#if showTimestamps}<span class="shrink-0 text-fg-subtle select-none">{formatTime(line.timestamp)}</span>{/if}
            <span>
              {#if line.kind === 'decoded'}<span class="text-fg-subtle select-none">↳ </span>{/if}
              {#if line.kind === 'sent'}<span class="select-none">› </span>{/if}
              {#each parseAnsi(line.text) as segment, j (j)}
                <span class={cn(segment.fg !== null && ANSI_CLASSES[segment.fg], segment.bold && 'font-semibold', segment.dim && 'opacity-70')}
                  >{#each highlight(segment.text, search) as part, k (k)}{#if part.match}<mark class="rounded-sm bg-warning/35 text-inherit">{part.text}</mark>{:else}{part.text}{/if}{/each}</span
                >
              {/each}
            </span>
          </div>
        {/each}
      </div>
    </div>
  </div>
  {#if !following && rows.length > 0}
    <button
      type="button"
      onclick={scrollToBottom}
      class="absolute right-4 bottom-3 inline-flex h-7 items-center gap-1.5 rounded-md border border-border-strong bg-surface-2 px-2.5 text-xs text-fg shadow-popover outline-none hover:bg-surface-3 focus-visible:ring-2 focus-visible:ring-focus/60"
    >
      <ArrowDownToLine class="size-3.5" /> Latest output
    </button>
  {/if}
</div>
