<script lang="ts">
  import { save as saveDialog } from '@tauri-apps/plugin-dialog';
  import { api } from '$lib/api';
  import type { PartitionTableView } from '$lib/bindings/PartitionTableView';
  import type { TableFormat } from '$lib/bindings/TableFormat';
  import JobPanel from '$lib/components/JobPanel.svelte';
  import PageHeader from '$lib/components/PageHeader.svelte';
  import PartitionMap from '$lib/components/PartitionMap.svelte';
  import PartitionTable from '$lib/components/PartitionTable.svelte';
  import ActionButton from '$lib/components/ui/ActionButton.svelte';
  import Badge from '$lib/components/ui/Badge.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Callout from '$lib/components/ui/Callout.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import ErrorCallout from '$lib/components/ui/ErrorCallout.svelte';
  import Field from '$lib/components/ui/Field.svelte';
  import FileField from '$lib/components/ui/FileField.svelte';
  import Input from '$lib/components/ui/Input.svelte';
  import Panel from '$lib/components/ui/Panel.svelte';
  import Select from '$lib/components/ui/Select.svelte';
  import ViewScroll from '$lib/components/ViewScroll.svelte';
  import { deviceBlocker } from '$lib/device';
  import { Download, TableProperties, Upload } from '$lib/icons';
  import { AUTO, FLASH_SIZES, flashSizeBytes } from '$lib/options';
  import { devices } from '$lib/stores/devices.svelte';
  import { JobRunner } from '$lib/stores/job.svelte';
  import { toasts } from '$lib/stores/toasts.svelte';
  import { type AppError, toAppError } from '$lib/utils/errors';
  import { basename, formatHex } from '$lib/utils/format';
  import { parseNumber } from '$lib/utils/parse';

  const job = new JobRunner();
  let filePath = $state<string | null>(null);
  let table = $state<PartitionTableView | null>(null);
  let source = $state<string | null>(null);
  let error = $state<AppError | null>(null);
  let deviceOffsetText = $state('0x8000');
  let sizeChoice = $state<string>(AUTO);

  const device = $derived(devices.active);
  const detectedSize = $derived(device?.chipInfo?.flashSize ? flashSizeBytes(device.chipInfo.flashSize) : null);
  const flashSize = $derived(sizeChoice === AUTO ? detectedSize : flashSizeBytes(sizeChoice as (typeof FLASH_SIZES)[number]));
  const deviceOffset = $derived(parseNumber(deviceOffsetText));

  async function loadFile(path: string | null) {
    filePath = path;
    error = null;
    if (!path) return;
    try {
      table = await api.loadPartitionTable(path, flashSize, null);
      source = basename(path);
    } catch (e) {
      table = null;
      error = toAppError(e);
    }
  }

  async function readFromDevice() {
    if (!device || !deviceOffset.ok) return;
    error = null;
    const offset = deviceOffset.value;
    const view = await job.run({ label: 'Reading partition table', deviceId: device.id, kind: 'readPartitionTable' }, (l) =>
      api.readPartitionTable({ deviceId: device.id, offset, connection: devices.connectionOptions() }, l),
    );
    if (view) {
      table = view;
      filePath = null;
      source = `${device.chipInfo?.chipName ?? 'Device'} at ${formatHex(offset)}`;
    }
  }

  async function exportTable(format: TableFormat) {
    if (!table) return;
    const extension = format === 'csv' ? 'csv' : 'bin';
    const path = await saveDialog({
      filters: [{ name: format === 'csv' ? 'CSV partition table' : 'Binary partition table', extensions: [extension] }],
      defaultPath: `partitions.${extension}`,
    });
    if (typeof path !== 'string') return;
    try {
      const result = await api.exportPartitionTable(table.entries, format, path);
      toasts.show('success', `Saved ${basename(result.outputPath)}`, `${result.size} bytes`, 4000);
    } catch (e) {
      toasts.show('danger', toAppError(e).message);
    }
  }

  const errors = $derived(table?.issues.filter((i) => i.severity === 'error') ?? []);
  const warnings = $derived(table?.issues.filter((i) => i.severity === 'warning') ?? []);
</script>

<ViewScroll>
  <PageHeader title="Partitions" description="Inspect, validate and convert ESP-IDF partition tables.">
    {#snippet actions()}
      <Button disabled={!table} onclick={() => exportTable('csv')}><Upload /> Export CSV</Button>
      <Button disabled={!table} onclick={() => exportTable('binary')}><Upload /> Export binary</Button>
    {/snippet}
  </PageHeader>

  <div class="grid grid-cols-1 gap-5 lg:grid-cols-2">
    <Panel title="Open a file" description="CSV (as used by ESP-IDF) or binary partition table.">
      <FileField
        path={filePath}
        filters={[{ name: 'Partition table', extensions: ['csv', 'bin'] }]}
        dropHint="Drop a .csv or .bin partition table here"
        aria-label="Choose a partition table file"
        onSelect={loadFile}
      />
    </Panel>
    <Panel title="Read from device" description="Reads the table stored in flash on the connected device.">
      <div class="flex flex-col gap-3">
        <div class="flex items-end gap-2">
          <Field label="Table offset" id="pt-offset" error={deviceOffset.ok ? null : deviceOffset.error} class="w-40">
            <Input id="pt-offset" bind:value={deviceOffsetText} mono invalid={!deviceOffset.ok} />
          </Field>
          <ActionButton
            disabledReason={job.running ? 'Reading…' : (deviceBlocker(device) ?? (!deviceOffset.ok ? 'Enter a valid offset.' : null))}
            onclick={readFromDevice}
          >
            <Download /> Read
          </ActionButton>
        </div>
        <JobPanel runner={job} />
      </div>
    </Panel>
  </div>

  {#if error}<ErrorCallout error={error} title="The partition table could not be loaded" />{/if}

  {#if table}
    <Panel title="Layout" description={source ?? undefined}>
      {#snippet actions()}
        <Badge>{table!.sourceFormat === 'csv' ? 'CSV' : 'Binary'}</Badge>
        <Badge>{table!.entries.length} partitions</Badge>
        <div class="w-40">
          <Select
            items={[
              { value: AUTO, label: detectedSize ? 'Detected flash size' : 'Flash size unknown' },
              ...FLASH_SIZES.map((s) => ({ value: s, label: `${s.replace(/(\d+)/, '$1 ')} flash` })),
            ]}
            value={sizeChoice}
            onValueChange={(v) => {
              sizeChoice = v;
              if (filePath) void loadFile(filePath);
            }}
            aria-label="Flash size used for layout checks"
          />
        </div>
      {/snippet}
      <div class="flex flex-col gap-4">
        <PartitionMap entries={table.entries} {flashSize} tableOffset={table.tableOffset} />
        {#if errors.length}
          <Callout tone="danger" title="Layout problems">
            <ul class="flex flex-col gap-0.5">
              {#each errors as issue, i (i)}<li><span class="font-mono text-xs text-fg">{issue.partition}</span>: {issue.message}</li>{/each}
            </ul>
          </Callout>
        {/if}
        {#if warnings.length}
          <Callout tone="warning" title="Warnings">
            <ul class="flex flex-col gap-0.5">
              {#each warnings as issue, i (i)}<li><span class="font-mono text-xs text-fg">{issue.partition}</span>: {issue.message}</li>{/each}
            </ul>
          </Callout>
        {/if}
        {#if !errors.length && !warnings.length}
          <p class="text-xs text-success">
            The table is valid: no overlaps, correct alignment{flashSize ? ', fits into the flash' : ''}.
          </p>
        {/if}
        <PartitionTable entries={table.entries} issues={table.issues} />
      </div>
    </Panel>
  {:else if !error}
    <Panel>
      <EmptyState
        icon={TableProperties}
        title="No partition table loaded"
        description="Open a CSV or binary file, or read the table from the connected device. Nothing is shown until a real table has been loaded."
      />
    </Panel>
  {/if}
</ViewScroll>
