<script lang="ts">
  import { api } from '$lib/api';
  import type { Chip } from '$lib/bindings/Chip';
  import type { ElfInfo } from '$lib/bindings/ElfInfo';
  import type { ImageOptions } from '$lib/bindings/ImageOptions';
  import type { SaveImageResult } from '$lib/bindings/SaveImageResult';
  import type { XtalFreq } from '$lib/bindings/XtalFreq';
  import ImageOptionsEditor from '$lib/components/ImageOptionsEditor.svelte';
  import PageHeader from '$lib/components/PageHeader.svelte';
  import PartitionMap from '$lib/components/PartitionMap.svelte';
  import ActionButton from '$lib/components/ui/ActionButton.svelte';
  import Badge from '$lib/components/ui/Badge.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Callout from '$lib/components/ui/Callout.svelte';
  import CopyButton from '$lib/components/ui/CopyButton.svelte';
  import ErrorCallout from '$lib/components/ui/ErrorCallout.svelte';
  import Field from '$lib/components/ui/Field.svelte';
  import FileField from '$lib/components/ui/FileField.svelte';
  import KeyValue from '$lib/components/ui/KeyValue.svelte';
  import Panel from '$lib/components/ui/Panel.svelte';
  import Select from '$lib/components/ui/Select.svelte';
  import Switch from '$lib/components/ui/Switch.svelte';
  import ViewScroll from '$lib/components/ViewScroll.svelte';
  import { CircleCheck, FileCog, FolderOpen, Zap } from '$lib/icons';
  import { AUTO, flashSizeBytes } from '$lib/options';
  import { app } from '$lib/stores/app.svelte';
  import { nav } from '$lib/stores/nav.svelte';
  import { settings } from '$lib/stores/settings.svelte';
  import { toasts } from '$lib/stores/toasts.svelte';
  import { type AppError, toAppError } from '$lib/utils/errors';
  import { basename, formatAddress, formatSize } from '$lib/utils/format';

  let elfPath = $state<string | null>(null);
  let elf = $state<ElfInfo | null>(null);
  let elfError = $state<AppError | null>(null);
  let chip = $state<Chip | null>(null);
  let xtal = $state<string>(AUTO);
  let merge = $state(false);
  let skipPadding = $state(false);
  let outputPath = $state<string | null>(null);
  let options = $state<ImageOptions>({
    flash: { mode: null, size: null, frequency: null },
    bootloader: null,
    partitionTable: null,
    partitionTableOffset: null,
    targetAppPartition: null,
    minChipRevision: null,
    mmuPageSize: null,
    checkAppDescriptor: true,
  });
  let optionErrors = $state<string[]>([]);
  let busy = $state(false);
  let result = $state<SaveImageResult | null>(null);
  let error = $state<AppError | null>(null);

  const descriptor = $derived(app.chip(chip));

  async function selectElf(path: string | null) {
    elfPath = path;
    elf = null;
    elfError = null;
    result = null;
    if (!path) return;
    try {
      elf = await api.inspectElf(path);
      if (elf.chip) chip = elf.chip;
    } catch (e) {
      elfError = toAppError(e);
    }
  }

  $effect(() => {
    // Reset the crystal selection when the chip changes.
    void chip;
    xtal = AUTO;
  });

  const chipConflict = $derived(
    elf?.chip && chip && elf.chip !== chip ? `The ELF was built for ${elf.chipName}, not ${descriptor?.name ?? chip}.` : null,
  );

  const blocker = $derived(
    busy
      ? 'Generating…'
      : !elfPath
        ? 'Select an ELF file.'
        : elfError
          ? 'The selected file is not a valid ELF.'
          : !chip
            ? 'Select the target chip.'
            : (chipConflict ??
              (optionErrors.length ? 'Correct the invalid options.' : null) ??
              (!outputPath ? 'Choose an output file.' : null)),
  );

  async function generate() {
    if (!elfPath || !chip || !outputPath) return;
    busy = true;
    error = null;
    result = null;
    try {
      result = await api.saveImage({
        elfPath,
        chip,
        xtal: xtal === AUTO ? null : (xtal as XtalFreq),
        options,
        merge,
        skipPadding: merge && skipPadding,
        outputPath,
      });
    } catch (e) {
      error = toAppError(e);
    } finally {
      busy = false;
    }
  }

  async function reveal(path: string) {
    try {
      await api.revealPath(path);
    } catch (e) {
      toasts.show('danger', toAppError(e).message);
    }
  }
</script>

<ViewScroll>
  <PageHeader
    title="Image Tools"
    description="Convert an ELF into a flashable ESP-IDF image. No device is required."
  />

  <div class="grid grid-cols-1 items-start gap-5 xl:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]">
    <div class="flex min-w-0 flex-col gap-5">
      <Panel title="Input">
        <div class="flex flex-col gap-4">
          <FileField
            path={elfPath}
            filters={[{ name: 'ELF executable', extensions: ['elf', 'out', '*'] }]}
            defaultPath={settings.value.firmwareDir}
            dropHint="Drop an ELF file here or browse"
            aria-label="Choose an ELF file"
            onSelect={selectElf}
          />
          {#if elfError}<ErrorCallout error={elfError} />{/if}
          {#if elf}
            <div class="flex flex-wrap gap-1.5">
              <Badge tone={elf.chipName ? 'accent' : 'neutral'}>{elf.chipName ? `Built for ${elf.chipName}` : 'Target chip not recorded'}</Badge>
              <Badge tone={elf.hasAppDescriptor ? 'success' : 'warning'}>
                {elf.hasAppDescriptor ? 'App descriptor present' : 'No app descriptor'}
              </Badge>
              <Badge>{formatSize(elf.file.size)}</Badge>
            </div>
          {/if}
          <div class="grid grid-cols-2 gap-3">
            <Field label="Target chip" id="img-chip" hint={elf?.chip ? 'Taken from the ELF metadata.' : undefined}>
              <Select
                id="img-chip"
                items={app.chips.map((c) => ({ value: c.chip, label: c.name }))}
                value={chip ?? ''}
                placeholder="Select a chip"
                onValueChange={(v) => (chip = v as Chip)}
              />
            </Field>
            <Field label="Crystal frequency" id="img-xtal" hint="Selects the matching bundled bootloader.">
              <Select
                id="img-xtal"
                disabled={!descriptor}
                items={[
                  { value: AUTO, label: descriptor ? `Default (${descriptor.defaultXtalFrequency.replace('MHz', ' MHz')})` : 'Default' },
                  ...(descriptor?.supportedXtalFrequencies ?? []).map((f) => ({ value: f, label: f.replace('MHz', ' MHz') })),
                ]}
                bind:value={xtal}
              />
            </Field>
          </div>
          {#if chipConflict}<Callout tone="danger">{chipConflict}</Callout>{/if}
        </div>
      </Panel>

      <Panel title="Image options">
        <ImageOptionsEditor bind:options bind:errors={optionErrors} {chip} sizeAutoLabel="Default (4 MB)" />
      </Panel>

      <Panel title="Output">
        <div class="flex flex-col gap-4">
          <Switch
            label="Merge into a single image"
            description="Combine bootloader, partition table and application into one file that is written at offset 0x0."
            bind:checked={merge}
          />
          <Switch
            label="Skip padding"
            description="Do not pad the merged image to the full flash size."
            bind:checked={skipPadding}
            disabled={!merge}
          />
          <FileField
            mode="save"
            bind:path={outputPath}
            filters={[{ name: 'Binary image', extensions: ['bin'] }]}
            defaultPath={settings.value.firmwareDir}
            suggestedName={elfPath ? `${basename(elfPath).replace(/\.[^.]+$/, '')}${merge ? '-merged' : ''}.bin` : 'firmware.bin'}
            placeholder="Choose an output file…"
            aria-label="Choose the output file"
          />
        </div>
      </Panel>
    </div>

    <div class="flex min-w-0 flex-col gap-4">
      <ActionButton variant="primary" class="w-full" loading={busy} disabledReason={blocker} onclick={generate}>
        <FileCog /> Generate image
      </ActionButton>
      {#if error}<ErrorCallout error={error} title="The image could not be generated" />{/if}
      {#if result}
        {@const summary = result.summary}
        <Panel title="Image generated">
          {#snippet actions()}<CircleCheck class="size-4 text-success" />{/snippet}
          <div class="flex flex-col gap-4">
            <KeyValue
              rows={[
                { label: 'File', value: basename(result.outputPath) },
                { label: 'Size', value: formatSize(result.outputSize) },
                { label: 'Write at', value: formatAddress(result.flashOffset), mono: true },
                { label: 'Chip', value: app.chip(summary.chip)?.name ?? summary.chip },
                { label: 'Flash', value: `${summary.flashSize} · ${summary.flashFrequency}${summary.flashMode ? ` · ${summary.flashMode.toUpperCase()}` : ''}` },
              ]}
            />
            <div class="flex items-center gap-1.5 rounded-md border border-border bg-surface-2 px-2.5 py-1.5">
              <span class="text-2xs font-medium text-fg-subtle">SHA-256</span>
              <span class="min-w-0 flex-1 truncate font-mono text-2xs text-fg">{result.sha256}</span>
              <CopyButton text={result.sha256} label="Copy SHA-256" />
            </div>
            <table class="w-full text-[13px]">
              <thead class="text-xs text-fg-subtle"><tr><th class="py-1 text-left font-medium">Segment</th><th class="py-1 text-right font-medium">Offset</th><th class="py-1 text-right font-medium">Size</th></tr></thead>
              <tbody class="divide-y divide-border">
                {#each summary.segments as segment (segment.offset)}
                  <tr>
                    <td class="py-1.5 text-fg-muted">{segment.label}</td>
                    <td class="py-1.5 text-right font-mono text-xs">{formatAddress(segment.offset)}</td>
                    <td class="py-1.5 text-right text-xs text-fg-subtle tabular">{formatSize(segment.size)}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
            {#if !merge}
              <p class="text-xs text-fg-subtle">
                Only the application image was saved. The device also needs a matching bootloader and partition table.
              </p>
            {/if}
            {#if summary.appPartitionSize > 0}
              <p class="text-xs text-fg-subtle">
                Application uses {formatSize(summary.appSize)} of its {formatSize(summary.appPartitionSize)} partition
                ({Math.round((summary.appSize / summary.appPartitionSize) * 100)}%).
              </p>
            {/if}
            <PartitionMap
              entries={summary.partitionTable.entries}
              flashSize={flashSizeBytes(summary.flashSize)}
              tableOffset={summary.partitionTable.tableOffset}
            />
            {#each summary.warnings as warning (warning)}<Callout tone="warning">{warning}</Callout>{/each}
            <div class="flex flex-wrap gap-2">
              <Button size="sm" onclick={() => reveal(result!.outputPath)}><FolderOpen /> Show in folder</Button>
              <Button size="sm" onclick={() => nav.openFlash({ mode: 'bin', path: result!.outputPath, offset: result!.flashOffset })}>
                <Zap /> Flash this image
              </Button>
            </div>
          </div>
        </Panel>
      {/if}
    </div>
  </div>
</ViewScroll>
