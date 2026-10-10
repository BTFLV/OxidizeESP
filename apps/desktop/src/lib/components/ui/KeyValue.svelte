<script lang="ts" module>
  export interface KeyValueRow {
    label: string;
    value: string | null | undefined;
    mono?: boolean;
    hint?: string;
  }
</script>

<script lang="ts">
  import { cn } from '$lib/utils/cn';

  let { rows, class: className }: { rows: KeyValueRow[]; class?: string } = $props();
</script>

<dl class={cn('grid grid-cols-[minmax(7rem,max-content)_1fr] gap-x-4 gap-y-1.5 text-[13px]', className)}>
  {#each rows as row (row.label)}
    <dt class="text-fg-subtle">{row.label}</dt>
    <dd class={cn('min-w-0 truncate text-fg selectable', row.mono && 'font-mono text-xs leading-5', !row.value && 'text-fg-subtle')} title={row.hint ?? row.value ?? undefined}>
      {row.value || 'Not available'}
    </dd>
  {/each}
</dl>
