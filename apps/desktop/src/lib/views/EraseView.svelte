<script lang="ts">
  import { api } from '$lib/api';
  import type { EraseResult } from '$lib/bindings/EraseResult';
  import type { PartitionSource } from '$lib/bindings/PartitionSource';
  import type { PartitionTableView } from '$lib/bindings/PartitionTableView';
  import JobPanel from '$lib/components/JobPanel.svelte';
  import PageHeader from '$lib/components/PageHeader.svelte';
  import PartitionTable from '$lib/components/PartitionTable.svelte';
  import TargetDevice from '$lib/components/TargetDevice.svelte';
  import ActionButton from '$lib/components/ui/ActionButton.svelte';
  import Callout from '$lib/components/ui/Callout.svelte';
  import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';
  import ErrorCallout from '$lib/components/ui/ErrorCallout.svelte';
  import Field from '$lib/components/ui/Field.svelte';
  import FileField from '$lib/components/ui/FileField.svelte';
  import Input from '$lib/components/ui/Input.svelte';
  import KeyValue from '$lib/components/ui/KeyValue.svelte';
  import Panel from '$lib/components/ui/Panel.svelte';
  import Segmented from '$lib/components/ui/Segmented.svelte';
  import ViewScroll from '$lib/components/ViewScroll.svelte';
  import { deviceBlocker } from '$lib/device';
  import { Download, Eraser } from '$lib/icons';
  import { flashSizeBytes } from '$lib/options';
  import { devices } from '$lib/stores/devices.svelte';
  import { JobRunner } from '$lib/stores/job.svelte';
  import { type AppError, toAppError } from '$lib/utils/errors';
  import { formatAddress, formatHex, formatSize } from '$lib/utils/format';
  import { checkRegion, parseNumber } from '$lib/utils/parse';

  type Target = 'region' | 'partitions' | 'full';
  let target = $state<Target>('region');
  const job = new JobRunner();
  const tableJob = new JobRunner();
  let result = $state<EraseResult | null>(null);
  let confirmOpen = $state(false);

  const device = $derived(devices.active);
  const info = $derived(device?.chipInfo ?? null);
  const blocker = $derived(deviceBlocker(device));
  const flashBytes = $derived(info?.flashSize ? flashSizeBytes(info.flashSize) : null);

  // Region
  let offsetText = $state('');
  let lengthText = $state('');
  const region = $derived(checkRegion(offsetText, lengthText, { alignOffset: true, alignLength: true, flashSize: flashBytes }));

  // Partitions
  let source = $state<'device' | 'file'>('device');
  let tableOffsetText = $state('0x8000');
  let tablePath = $state<string | null>(null);
  let table = $state<PartitionTableView | null>(null);
  let tableError = $state<AppError | null>(null);
  let selected = $state<string[]>([]);
  const tableOffset = $derived(parseNumber(tableOffsetText));

  async function loadTable() {
    table = null;
    tableError = null;
    selected = [];
    if (source === 'file') {
      if (!tablePath) return;
      try {
        table = await api.loadPartitionTable(tablePath, flashBytes, null);
      } catch (error) {
        tableError = toAppError(error);
      }
      return;
    }
    if (!device || !tableOffset.ok) return;
    const offset = tableOffset.value;
    const view = await tableJob.run({ label: 'Reading partition table', deviceId: device.id, kind: 'readPartitionTable' }, (l) =>
      api.readPartitionTable({ deviceId: device.id, offset, connection: devices.connectionOptions() }, l),
    );
    if (view) table = view;
  }

  const selectedEntries = $derived(table ? table.entries.filter((e) => selected.includes(e.name)) : []);

  const targetBlocker = $derived.by(() => {
    if (job.running || tableJob.running) return 'An operation is in progress.';
    if (blocker) return blocker;
    if (target === 'region') return region.valid ? null : 'Enter a valid, sector-aligned region.';
    if (target === 'partitions') return selectedEntries.length ? null : 'Load a partition table and select partitions.';
    return null;
  });

  async function run() {
    if (!device) return;
    result = null;
    const connection = devices.connectionOptions();
    if (target === 'full') {
      const r = await job.run({ label: 'Erasing entire flash', deviceId: device.id, kind: 'eraseFlash' }, (l) =>
        api.erase({ deviceId: device.id, target: { type: 'full' }, connection, confirmed: true }, l),
      );
      if (r) result = r;
    } else if (target === 'region' && region.offset.ok && region.length.ok) {
      const eraseRegion = { offset: region.offset.value, length: region.length.value };
      const r = await job.run({ label: 'Erasing region', deviceId: device.id, kind: 'eraseRegion' }, (l) =>
        api.erase(
          {
            deviceId: device.id,
            target: { type: 'region', region: eraseRegion },
            connection,
            confirmed: true,
          },
          l,
        ),
      );
      if (r) result = r;
    } else if (target === 'partitions') {
      const parsedOffset = tableOffset;
      const partitionSource: PartitionSource =
        source === 'file' && tablePath
          ? { type: 'file', path: tablePath }
          : { type: 'device', offset: parsedOffset.ok ? parsedOffset.value : 0x8000 };
      const r = await job.run({ label: 'Erasing partitions', deviceId: device.id, kind: 'erasePartitions' }, (l) =>
        api.erase(
          { deviceId: device.id, target: { type: 'partitions', names: selected, source: partitionSource }, connection, confirmed: true },
          l,
        ),
      );
      if (r) result = r;
    }
  }

  const confirmTitle = $derived(
    target === 'full' ? 'Erase the entire flash?' : target === 'region' ? 'Erase flash region?' : 'Erase selected partitions?',
  );
  const confirmDescription = $derived(
    target === 'full'
      ? 'Every byte of flash will be erased, including the bootloader, partition table, application and all stored data such as Wi-Fi credentials and calibration. The device will not boot until new firmware is flashed. This cannot be undone.'
      : 'The data in the selected region will be permanently erased. This cannot be undone.',
  );
</script>

<ViewScroll>
  <PageHeader title="Erase" description="Erase flash regions, partitions or the entire chip." />

  <div class="grid grid-cols-1 items-start gap-5 xl:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]">
    <div class="flex min-w-0 flex-col gap-5">
      <Panel title="What to erase">
        <div class="flex flex-col gap-4">
          <Segmented
            label="Erase target"
            options={[
              { value: 'region', label: 'Region' },
              { value: 'partitions', label: 'Partitions' },
              { value: 'full', label: 'Entire flash' },
            ]}
            bind:value={target}
            class="self-start"
          />

          {#if target === 'region'}
            <div class="grid grid-cols-2 gap-3">
              <Field label="Start address" id="erase-offset" error={offsetText.trim() && !region.offset.ok ? region.offset.error : null}>
                <Input id="erase-offset" bind:value={offsetText} placeholder="0x9000" mono invalid={!!offsetText.trim() && !region.offset.ok} />
              </Field>
              <Field
                label="Length"
                id="erase-length"
                error={lengthText.trim() ? (region.length.ok ? region.region : region.length.error) : null}
              >
                <Input id="erase-length" bind:value={lengthText} placeholder="0x6000" mono invalid={!!lengthText.trim() && !region.valid} />
              </Field>
            </div>
            <p class="text-xs text-fg-subtle">
              Flash is erased in 4 KiB sectors, so both values must be multiples of 0x1000.
              {#if region.valid && region.offset.ok && region.length.ok}
                Erases {formatAddress(region.offset.value)} – {formatAddress(region.offset.value + region.length.value)} ({formatSize(region.length.value)}).
              {/if}
            </p>
          {:else if target === 'partitions'}
            <Segmented
              label="Partition table source"
              options={[
                { value: 'device', label: 'Read from device' },
                { value: 'file', label: 'From file' },
              ]}
              bind:value={source}
              onValueChange={() => {
                table = null;
                selected = [];
              }}
              class="self-start"
            />
            {#if source === 'device'}
              <div class="flex items-end gap-2">
                <Field label="Table offset" id="erase-table-offset" error={tableOffset.ok ? null : tableOffset.error} class="w-40">
                  <Input id="erase-table-offset" bind:value={tableOffsetText} mono invalid={!tableOffset.ok} />
                </Field>
                <ActionButton
                  disabledReason={tableJob.running ? 'Reading…' : (blocker ?? (!tableOffset.ok ? 'Enter a valid offset.' : null))}
                  onclick={loadTable}
                >
                  <Download /> Read partition table
                </ActionButton>
              </div>
              <JobPanel runner={tableJob} />
            {:else}
              <FileField
                bind:path={tablePath}
                filters={[{ name: 'Partition table', extensions: ['csv', 'bin'] }]}
                aria-label="Choose a partition table"
                onSelect={() => loadTable()}
              />
              <p class="text-xs text-fg-subtle">Make sure the file matches the table currently flashed on the device.</p>
            {/if}
            {#if tableError}<ErrorCallout error={tableError} />{/if}
            {#if table}
              <PartitionTable entries={table.entries} issues={table.issues} selectable bind:selected />
            {/if}
          {:else}
            <Callout tone="danger" title="Erases everything">
              All flash contents are erased: bootloader, partition table, application, NVS data such as Wi-Fi credentials,
              and file systems. Consider creating a backup first.
            </Callout>
          {/if}
        </div>
      </Panel>
    </div>

    <div class="flex min-w-0 flex-col gap-4">
      <TargetDevice />
      <ActionButton variant="danger" class="w-full" disabledReason={targetBlocker} onclick={() => (confirmOpen = true)}>
        <Eraser />
        {target === 'full' ? 'Erase entire flash…' : target === 'region' ? 'Erase region…' : 'Erase partitions…'}
      </ActionButton>
      <JobPanel runner={job}>
        {#snippet success()}
          {#if result?.regions}
            <KeyValue
              rows={result.regions.map((r, i) => ({
                label: result?.partitions[i]?.name ?? `Region ${i + 1}`,
                value: `${formatHex(r.offset)} + ${formatHex(r.length)}`,
                mono: true,
              }))}
            />
          {/if}
        {/snippet}
      </JobPanel>
    </div>
  </div>
</ViewScroll>

<ConfirmDialog
  bind:open={confirmOpen}
  title={confirmTitle}
  description={confirmDescription}
  confirmLabel={target === 'full' ? 'Erase entire flash' : 'Erase'}
  requireText={target === 'full' ? 'ERASE' : null}
  onConfirm={run}
>
  {#snippet details()}
    <KeyValue
      rows={[
        { label: 'Device', value: device ? `${info?.chipName ?? 'Device'} on ${device.port.name}` : null },
        ...(target === 'full'
          ? [{ label: 'Range', value: flashBytes ? `${formatAddress(0)} – ${formatAddress(flashBytes)} (${formatSize(flashBytes)})` : 'Entire chip', mono: true }]
          : target === 'region' && region.offset.ok && region.length.ok
            ? [
                {
                  label: 'Range',
                  value: `${formatAddress(region.offset.value)} – ${formatAddress(region.offset.value + region.length.value)}`,
                  mono: true,
                },
              ]
            : selectedEntries.map((e) => ({ label: e.name, value: `${formatHex(e.offset)} · ${formatSize(e.size)}`, mono: true }))),
      ]}
    />
  {/snippet}
</ConfirmDialog>
