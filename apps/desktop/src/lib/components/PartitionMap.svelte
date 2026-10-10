<script lang="ts">
  import type { PartitionEntry } from '$lib/bindings/PartitionEntry';
  import { buildMap, CATEGORY_COLORS, CATEGORY_LABELS, mapExtent, type PartitionCategory } from '$lib/partitions';
  import { cn } from '$lib/utils/cn';
  import { formatHex, formatSize } from '$lib/utils/format';

  let {
    entries,
    flashSize,
    tableOffset = 0x8000,
  }: { entries: PartitionEntry[]; flashSize: number | null; tableOffset?: number } = $props();

  const extent = $derived(mapExtent(entries, flashSize, tableOffset));
  const segments = $derived(buildMap(entries, tableOffset));
  const used = $derived(Math.max(tableOffset + 0x1000, ...entries.map((e) => e.offset + e.size)));
  const categories = $derived([...new Set(segments.filter((s) => s.kind !== 'reserved').map((s) => s.kind))] as PartitionCategory[]);
</script>

<div class="flex flex-col gap-2">
  <div class="relative h-9 w-full overflow-hidden rounded-md border border-border bg-surface-2" role="img" aria-label="Flash memory map">
    {#each segments as segment (segment.key)}
      <div
        class={cn(
          'absolute inset-y-0 border-r border-surface',
          segment.kind === 'reserved'
            ? 'bg-[repeating-linear-gradient(135deg,var(--surface-3)_0_4px,transparent_4px_8px)]'
            : cn(CATEGORY_COLORS[segment.kind], 'opacity-85 hover:opacity-100'),
        )}
        style:left="{(segment.offset / extent) * 100}%"
        style:width="max(2px, {(segment.size / extent) * 100}%)"
        title="{segment.label}: {formatHex(segment.offset)}–{formatHex(segment.offset + segment.size)} ({formatSize(segment.size)})"
      ></div>
    {/each}
  </div>
  <div class="flex flex-wrap items-center justify-between gap-x-4 gap-y-1 text-2xs text-fg-subtle">
    <div class="flex flex-wrap items-center gap-x-3 gap-y-1">
      <span class="inline-flex items-center gap-1.5">
        <span class="size-2 rounded-sm bg-[repeating-linear-gradient(135deg,var(--fg-subtle)_0_2px,transparent_2px_4px)]"></span>
        Reserved
      </span>
      {#each categories as category (category)}
        <span class="inline-flex items-center gap-1.5">
          <span class={cn('size-2 rounded-sm', CATEGORY_COLORS[category])}></span>
          {CATEGORY_LABELS[category]}
        </span>
      {/each}
    </div>
    <span class="tabular">
      {formatSize(used)} used{#if flashSize}&nbsp;of {formatSize(flashSize)}{:else}&nbsp;(flash size unknown){/if}
    </span>
  </div>
</div>
