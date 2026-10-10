<script lang="ts">
  import type { PartitionEntry } from '$lib/bindings/PartitionEntry';
  import type { PartitionIssue } from '$lib/bindings/PartitionIssue';
  import { categorize, CATEGORY_COLORS } from '$lib/partitions';
  import { cn } from '$lib/utils/cn';
  import { formatHex, formatSize } from '$lib/utils/format';
  import Checkbox from './ui/Checkbox.svelte';

  let {
    entries,
    issues = [],
    selectable = false,
    selected = $bindable([]),
  }: {
    entries: PartitionEntry[];
    issues?: PartitionIssue[];
    selectable?: boolean;
    selected?: string[];
  } = $props();

  function toggle(name: string, checked: boolean) {
    selected = checked ? [...new Set([...selected, name])] : selected.filter((n) => n !== name);
  }
</script>

<div class="overflow-x-auto rounded-md border border-border">
  <table class="w-full text-left text-[13px]">
    <thead class="bg-surface-2 text-xs text-fg-subtle">
      <tr>
        {#if selectable}<th class="w-8 px-3 py-2"><span class="sr-only">Select</span></th>{/if}
        <th class="px-3 py-2 font-medium">Name</th>
        <th class="px-3 py-2 font-medium">Type</th>
        <th class="px-3 py-2 font-medium">Subtype</th>
        <th class="px-3 py-2 text-right font-medium">Offset</th>
        <th class="px-3 py-2 text-right font-medium">Size</th>
        <th class="px-3 py-2 font-medium">Flags</th>
      </tr>
    </thead>
    <tbody class="divide-y divide-border">
      {#each entries as entry (entry.name)}
        {@const entryIssues = issues.filter((i) => i.partition === entry.name)}
        <tr class={cn(entryIssues.some((i) => i.severity === 'error') && 'bg-danger-soft')}>
          {#if selectable}
            <td class="px-3 py-1.5">
              <Checkbox
                label={`Select ${entry.name}`}
                hideLabel
                checked={selected.includes(entry.name)}
                onCheckedChange={(v) => toggle(entry.name, v)}
              />
            </td>
          {/if}
          <td class="px-3 py-1.5">
            <span class="inline-flex items-center gap-2">
              <span class={cn('size-2 rounded-sm', CATEGORY_COLORS[categorize(entry)])} aria-hidden="true"></span>
              <span class="font-mono text-xs text-fg">{entry.name}</span>
            </span>
          </td>
          <td class="px-3 py-1.5 text-fg-muted">{entry.typeName}</td>
          <td class="px-3 py-1.5 text-fg-muted">{entry.subtypeName}</td>
          <td class="px-3 py-1.5 text-right font-mono text-xs text-fg tabular">{formatHex(entry.offset)}</td>
          <td class="px-3 py-1.5 text-right text-fg tabular">
            <span class="font-mono text-xs">{formatHex(entry.size)}</span>
            <span class="ml-1 text-xs text-fg-subtle">({formatSize(entry.size)})</span>
          </td>
          <td class="px-3 py-1.5 text-xs text-fg-muted">
            {[entry.encrypted && 'encrypted', entry.readonly && 'readonly'].filter(Boolean).join(', ') || '—'}
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>
