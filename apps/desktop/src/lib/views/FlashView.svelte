<script lang="ts">
  import { Tabs } from 'bits-ui';
  import { api } from '$lib/api';
  import type { ConnectionOptions } from '$lib/bindings/ConnectionOptions';
  import type { ElfInfo } from '$lib/bindings/ElfInfo';
  import type { FileInfo } from '$lib/bindings/FileInfo';
  import type { FlashResult } from '$lib/bindings/FlashResult';
  import type { ImageOptions } from '$lib/bindings/ImageOptions';
  import type { ImageSummary } from '$lib/bindings/ImageSummary';
  import ConnectionSettings from '$lib/components/ConnectionSettings.svelte';
  import ImageOptionsEditor from '$lib/components/ImageOptionsEditor.svelte';
  import JobPanel from '$lib/components/JobPanel.svelte';
  import PageHeader from '$lib/components/PageHeader.svelte';
  import PartitionMap from '$lib/components/PartitionMap.svelte';
  import TargetDevice from '$lib/components/TargetDevice.svelte';
  import ActionButton from '$lib/components/ui/ActionButton.svelte';
  import Badge from '$lib/components/ui/Badge.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Callout from '$lib/components/ui/Callout.svelte';
  import Checkbox from '$lib/components/ui/Checkbox.svelte';
  import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';
  import ErrorCallout from '$lib/components/ui/ErrorCallout.svelte';
  import Field from '$lib/components/ui/Field.svelte';
  import FileField from '$lib/components/ui/FileField.svelte';
  import Input from '$lib/components/ui/Input.svelte';
  import KeyValue from '$lib/components/ui/KeyValue.svelte';
  import Panel from '$lib/components/ui/Panel.svelte';
  import Spinner from '$lib/components/ui/Spinner.svelte';
  import Switch from '$lib/components/ui/Switch.svelte';
  import TabBar from '$lib/components/ui/TabBar.svelte';
  import ViewScroll from '$lib/components/ViewScroll.svelte';
  import { deviceBlocker } from '$lib/device';
  import { Binary, ChevronDown, ChevronRight, FileCode, Terminal, Zap } from '$lib/icons';
  import { flashSizeBytes } from '$lib/options';
  import { app } from '$lib/stores/app.svelte';
  import { devices } from '$lib/stores/devices.svelte';
  import { JobRunner } from '$lib/stores/job.svelte';
  import { nav } from '$lib/stores/nav.svelte';
  import { settings } from '$lib/stores/settings.svelte';
  import { type AppError, toAppError } from '$lib/utils/errors';
  import { basename, formatAddress, formatHex, formatSize } from '$lib/utils/format';
  import { checkOffset } from '$lib/utils/parse';

  type Mode = 'elf' | 'bin';
  let mode = $state<Mode>('elf');
  const job = new JobRunner();
  let lastResult = $state<FlashResult | null>(null);
  let lastElf = $state<string | null>(null);

  const device = $derived(devices.active);
  const info = $derived(device?.chipInfo ?? null);
  const blocker = $derived(deviceBlocker(device));
  const flashBytes = $derived(info?.flashSize ? flashSizeBytes(info.flashSize) : null);

  let showAdvanced = $state(false);
  let connection = $state<ConnectionOptions>(devices.connectionOptions());
  let verify = $state(settings.value.verifyAfterFlash);
  let skipUnchanged = $state(false);

  $effect(() => {
    if (!settings.loaded) return;
    connection = devices.connectionOptions();
    verify = settings.value.verifyAfterFlash;
    showAdvanced = settings.value.showAdvanced;
  });

  // --- Application (ELF) ---------------------------------------------------
  let elfPath = $state<string | null>(null);
  let elf = $state<ElfInfo | null>(null);
  let elfError = $state<AppError | null>(null);
  let imageOptions = $state<ImageOptions>({
    flash: { mode: null, size: null, frequency: null },
    bootloader: null,
    partitionTable: null,
    partitionTableOffset: null,
    targetAppPartition: null,
    minChipRevision: null,
    mmuPageSize: null,
    checkAppDescriptor: true,
  });
  let imageErrors = $state<string[]>([]);
  let erasePartitionsText = $state('');
  let monitorAfter = $state(false);
  let preview = $state<ImageSummary | null>(null);
  let previewError = $state<AppError | null>(null);
  let previewLoading = $state(false);

  async function selectElf(path: string | null) {
    elfPath = path;
    elf = null;
    elfError = null;
    if (!path) return;
    try {
      elf = await api.inspectElf(path);
    } catch (error) {
      elfError = toAppError(error);
    }
  }

  const erasePartitions = $derived(
    erasePartitionsText
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean),
  );

  const chipMismatch = $derived(
    elf?.chipName && info && elf.chip !== info.chip
      ? `The firmware was built for ${elf.chipName}, but the device is an ${info.chipName}.`
      : null,
  );

  // Preview the image layout whenever its inputs change.
  $effect(() => {
    const path = elfPath;
    const chipInfo = info;
    const options = $state.snapshot(imageOptions) as ImageOptions;
    if (!path || !elf || !chipInfo || chipMismatch || imageErrors.length) {
      preview = null;
      previewError = null;
      return;
    }
    let cancelled = false;
    previewLoading = true;
    const timer = setTimeout(async () => {
      try {
        const summary = await api.previewImage({
          elfPath: path,
          chip: chipInfo.chip,
          xtal: chipInfo.crystal,
          options,
          flashSize: chipInfo.flashSize,
          revision: chipInfo.revisionNumber,
        });
        if (!cancelled) {
          preview = summary;
          previewError = null;
        }
      } catch (error) {
        if (!cancelled) {
          preview = null;
          previewError = toAppError(error);
        }
      } finally {
        if (!cancelled) previewLoading = false;
      }
    }, 250);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  });

  const elfBlocker = $derived(
    job.running
      ? 'An operation is in progress.'
      : !elfPath
        ? 'Select an ELF file.'
        : elfError
          ? 'The selected file is not a valid ELF.'
          : (blocker ??
            chipMismatch ??
            (imageErrors.length ? 'Correct the invalid image options.' : null) ??
            (previewError ? 'Resolve the image error first.' : null) ??
            (!preview ? 'Preparing the image layout…' : null)),
  );

  // --- Raw binary -------------------------------------------------------------
  let binPath = $state<string | null>(null);
  let binInfo = $state<FileInfo | null>(null);
  let binError = $state<AppError | null>(null);
  let offsetText = $state('');

  async function selectBin(path: string | null, offset: number | null = null) {
    binPath = path;
    binInfo = null;
    binError = null;
    offsetText = offset !== null ? formatHex(offset) : '';
    if (!path) return;
    try {
      binInfo = await api.inspectFile(path);
      if (offset === null && binInfo.backup) offsetText = '';
    } catch (error) {
      binError = toAppError(error);
    }
  }

  const offset = $derived(offsetText.trim() === '' ? null : checkOffset(offsetText, { align: true }));
  const binEnd = $derived(offset?.ok && binInfo ? offset.value + binInfo.size : null);
  const binRangeError = $derived(
    binEnd !== null && flashBytes !== null && binEnd > flashBytes
      ? `The file ends at ${formatHex(binEnd)}, beyond the ${formatSize(flashBytes)} flash.`
      : null,
  );

  const suggestions = $derived.by(() => {
    const list: { label: string; offset: number }[] = [];
    if (binInfo?.backup) list.push({ label: 'Original backup offset', offset: binInfo.backup.flashOffset });
    const descriptor = app.chip(info?.chip);
    if (descriptor) list.push({ label: 'Bootloader', offset: descriptor.bootloaderOffset });
    list.push({ label: 'Partition table', offset: 0x8000 }, { label: 'Default app partition', offset: 0x10000 });
    return list;
  });

  const kindWarning = $derived.by(() => {
    if (!binInfo || !offset?.ok) return null;
    if (binInfo.kind === 'elf') return 'This is an ELF file. Use the Application tab to flash ELF files.';
    if (binInfo.kind === 'partitionTable' && offset.value !== 0x8000)
      return 'The file is a partition table but will be written somewhere other than the default 0x8000.';
    if (binInfo.backup && binInfo.backup.flashOffset !== offset.value)
      return `This backup was read from ${formatHex(binInfo.backup.flashOffset)}.`;
    return null;
  });

  const binBlocker = $derived(
    job.running
      ? 'An operation is in progress.'
      : !binPath
        ? 'Select a file to write.'
        : binError
          ? 'The selected file cannot be read.'
          : binInfo?.kind === 'elf'
            ? 'ELF files must be flashed from the Application tab.'
            : !offset
              ? 'Enter the flash offset to write to.'
              : !offset.ok
                ? 'Correct the flash offset.'
                : (binRangeError ?? blocker),
  );

  // --- Intents from other views ------------------------------------------------
  $effect(() => {
    const intent = nav.flashIntent;
    if (!intent) return;
    nav.flashIntent = null;
    if (intent.mode === 'elf') {
      mode = 'elf';
      void selectElf(intent.path);
    } else {
      mode = 'bin';
      void selectBin(intent.path, intent.offset);
    }
  });

  // --- Run ---------------------------------------------------------------------
  let confirmOpen = $state(false);

  async function run() {
    if (!device) return;
    lastResult = null;
    if (mode === 'elf' && elfPath) {
      const path = elfPath;
      const result = await job.run({ label: 'Flashing application', deviceId: device.id, kind: 'flashElf' }, (l) =>
        api.flashElf(
          {
            deviceId: device.id,
            elfPath: path,
            connection,
            image: imageOptions,
            verify,
            skipUnchanged,
            erasePartitions,
            confirmed: true,
          },
          l,
        ),
      );
      if (result) {
        lastResult = result;
        lastElf = path;
        if (monitorAfter) nav.openMonitor({ deviceId: device.id, elfPath: path, autoStart: true });
      }
    } else if (mode === 'bin' && binPath && offset?.ok) {
      const result = await job.run({ label: 'Writing binary', deviceId: device.id, kind: 'writeBin' }, (l) =>
        api.writeBin(
          {
            deviceId: device.id,
            path: binPath!,
            offset: offset.value,
            connection,
            verify,
            skipUnchanged,
            confirmed: true,
          },
          l,
        ),
      );
      if (result) {
        lastResult = result;
        lastElf = null;
      }
    }
  }

  const currentBlocker = $derived(mode === 'elf' ? elfBlocker : binBlocker);
</script>

<ViewScroll>
  <PageHeader title="Flash" description="Write firmware to the connected device." />

  <Tabs.Root value={mode} onValueChange={(v) => (mode = v as Mode)} class="flex flex-col gap-5">
    <TabBar
      items={[
        { value: 'elf', label: 'Application (ELF)', icon: FileCode },
        { value: 'bin', label: 'Raw binary', icon: Binary },
      ]}
      class="self-start"
    />

    <div class="grid grid-cols-1 items-start gap-5 xl:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]">
      <div class="flex min-w-0 flex-col gap-5">
        <Tabs.Content value="elf" class="flex flex-col gap-5">
          <Panel
            title="Application"
            description="An ELF built for an ESP32-family chip. espflash creates the bootloader, partition table and application image."
          >
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
                  <Badge tone={elf.chipName ? 'accent' : 'neutral'}>
                    {elf.chipName ? `Built for ${elf.chipName}` : 'Target chip not recorded'}
                  </Badge>
                  <Badge tone={elf.hasAppDescriptor ? 'success' : 'warning'}>
                    {elf.hasAppDescriptor ? 'App descriptor present' : 'No app descriptor'}
                  </Badge>
                  {#if elf.isEspHal}<Badge>esp-hal</Badge>{/if}
                  {#if elf.minChipRevision}
                    <Badge>Min. revision v{Math.floor(elf.minChipRevision / 100)}.{elf.minChipRevision % 100}</Badge>
                  {/if}
                  <Badge>{formatSize(elf.file.size)}</Badge>
                </div>
                {#if chipMismatch}<Callout tone="danger" title="Incompatible firmware">{chipMismatch}</Callout>{/if}
                {#if !elf.hasAppDescriptor && imageOptions.checkAppDescriptor}
                  <Callout tone="warning">
                    espflash requires an ESP-IDF application descriptor by default. Add it to the firmware or disable the
                    check in the advanced options.
                  </Callout>
                {/if}
              {/if}
            </div>
          </Panel>

          <Panel title="Options">
            <div class="flex flex-col gap-3">
              <Switch label="Verify after writing" description="Compare a device-side MD5 checksum with the written data." bind:checked={verify} />
              <Switch label="Skip unchanged regions" description="Skip segments whose contents already match." bind:checked={skipUnchanged} />
              <Checkbox label="Open the serial monitor after flashing" bind:checked={monitorAfter} />
            </div>
          </Panel>

          <Panel title="Advanced" bodyClass={showAdvanced ? 'p-4' : 'p-0'}>
            {#snippet actions()}
              <Button variant="ghost" size="sm" onclick={() => (showAdvanced = !showAdvanced)} aria-expanded={showAdvanced}>
                {#if showAdvanced}<ChevronDown />{:else}<ChevronRight />{/if}
                {showAdvanced ? 'Hide' : 'Show'}
              </Button>
            {/snippet}
            {#if showAdvanced}
              <div class="flex flex-col gap-5">
                <ImageOptionsEditor bind:options={imageOptions} bind:errors={imageErrors} chip={info?.chip ?? elf?.chip ?? null} />
                <Field
                  label="Erase partitions before flashing"
                  id="erase-parts"
                  hint="Comma-separated partition labels from the image's partition table, for example nvs, otadata."
                >
                  <Input id="erase-parts" bind:value={erasePartitionsText} placeholder="nvs" mono />
                </Field>
                <div class="border-t border-border pt-4">
                  <ConnectionSettings bind:options={connection} />
                </div>
              </div>
            {/if}
          </Panel>
        </Tabs.Content>

        <Tabs.Content value="bin" class="flex flex-col gap-5">
          <Panel title="Binary file" description="Written verbatim to the flash offset you specify.">
            <div class="flex flex-col gap-4">
              <FileField
                path={binPath}
                filters={[{ name: 'Binary image', extensions: ['bin', 'img', '*'] }]}
                defaultPath={settings.value.firmwareDir}
                aria-label="Choose a binary file"
                onSelect={(p) => selectBin(p)}
              />
              {#if binError}<ErrorCallout error={binError} />{/if}
              {#if binInfo}
                <KeyValue
                  rows={[
                    { label: 'Size', value: `${formatSize(binInfo.size)} (${binInfo.size.toLocaleString('en-US')} bytes)` },
                    {
                      label: 'Detected type',
                      value: {
                        elf: 'ELF executable',
                        espImage: 'ESP image (bootloader or application)',
                        partitionTable: 'Binary partition table',
                        raw: 'Raw data',
                      }[binInfo.kind],
                    },
                    { label: 'SHA-256', value: binInfo.sha256, mono: true },
                    ...(binInfo.backup
                      ? [
                          {
                            label: 'Backup of',
                            value: `${app.chip(binInfo.backup.chip)?.name ?? binInfo.backup.chip} · ${formatHex(binInfo.backup.flashOffset)}–${formatHex(binInfo.backup.flashOffset + binInfo.backup.length)}`,
                          },
                        ]
                      : []),
                  ]}
                />
              {/if}

              <Field
                label="Flash offset"
                id="bin-offset"
                error={offset && !offset.ok ? offset.error : binRangeError}
                hint="Required. There is no universally correct offset for raw binaries."
              >
                <Input id="bin-offset" bind:value={offsetText} placeholder="0x10000" mono invalid={!!offset && !offset.ok} />
              </Field>
              <div class="flex flex-wrap items-center gap-1.5">
                <span class="text-xs text-fg-subtle">Common offsets:</span>
                {#each suggestions as s (s.label)}
                  <Button size="sm" variant="outline" onclick={() => (offsetText = formatHex(s.offset))}>
                    {s.label} <span class="font-mono text-fg-subtle">{formatHex(s.offset)}</span>
                  </Button>
                {/each}
              </div>
              {#if kindWarning}<Callout tone="warning">{kindWarning}</Callout>{/if}
              {#if binInfo?.backup?.flashEncryption}
                <Callout tone="warning">
                  This backup was taken from a device with flash encryption enabled. It can only be restored to that device.
                </Callout>
              {/if}
            </div>
          </Panel>

          <Panel title="Options">
            <div class="flex flex-col gap-3">
              <Switch label="Verify after writing" bind:checked={verify} />
              <Switch label="Skip if unchanged" bind:checked={skipUnchanged} />
            </div>
          </Panel>

          <Panel title="Connection" bodyClass={showAdvanced ? 'p-4' : 'p-0'}>
            {#snippet actions()}
              <Button variant="ghost" size="sm" onclick={() => (showAdvanced = !showAdvanced)} aria-expanded={showAdvanced}>
                {#if showAdvanced}<ChevronDown />{:else}<ChevronRight />{/if}
                {showAdvanced ? 'Hide' : 'Show'}
              </Button>
            {/snippet}
            {#if showAdvanced}<ConnectionSettings bind:options={connection} />{/if}
          </Panel>
        </Tabs.Content>
      </div>

      <div class="flex min-w-0 flex-col gap-4 xl:sticky xl:top-0">
        <TargetDevice />

        <Panel title="Summary">
          {#if mode === 'elf'}
            {#if preview}
              <div class="flex flex-col gap-3">
                <table class="w-full text-[13px]">
                  <tbody class="divide-y divide-border">
                    {#each preview.segments as segment (segment.offset)}
                      <tr>
                        <td class="py-1.5 text-fg-muted">{segment.label}</td>
                        <td class="py-1.5 text-right font-mono text-xs">{formatAddress(segment.offset)}</td>
                        <td class="py-1.5 text-right text-xs text-fg-subtle tabular">{formatSize(segment.size)}</td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
                {#if preview.appPartitionSize > 0}
                  <p class="text-xs text-fg-subtle">
                    Application uses {formatSize(preview.appSize)} of its {formatSize(preview.appPartitionSize)} partition
                    ({Math.round((preview.appSize / preview.appPartitionSize) * 100)}%).
                  </p>
                {/if}
                <PartitionMap
                  entries={preview.partitionTable.entries}
                  flashSize={flashBytes}
                  tableOffset={preview.partitionTable.tableOffset}
                />
                {#each preview.warnings as warning (warning)}<Callout tone="warning">{warning}</Callout>{/each}
              </div>
            {:else if previewLoading && elfPath && info}
              <p class="flex items-center gap-2 text-[13px] text-fg-subtle"><Spinner class="size-3.5" /> Preparing image…</p>
            {:else if previewError}
              <ErrorCallout error={previewError} title="The image cannot be built" />
            {:else}
              <p class="text-[13px] text-fg-subtle">
                {!info ? 'Identify the device to see where the image will be written.' : 'Select an ELF file.'}
              </p>
            {/if}
          {:else}
            <KeyValue
              rows={[
                { label: 'File', value: binPath ? basename(binPath) : null },
                { label: 'Start', value: offset?.ok ? formatAddress(offset.value) : null, mono: true },
                { label: 'End', value: binEnd !== null ? formatAddress(binEnd) : null, mono: true },
                { label: 'Size', value: binInfo ? formatSize(binInfo.size) : null },
              ]}
            />
          {/if}
        </Panel>

        <ActionButton variant="primary" disabledReason={currentBlocker} onclick={() => (confirmOpen = true)} class="w-full">
          <Zap />
          {mode === 'elf' ? 'Flash application' : 'Write binary'}
        </ActionButton>

        <JobPanel runner={job}>
          {#snippet success()}
            {#if lastResult}
              <KeyValue
                rows={[
                  { label: 'Written', value: formatSize(lastResult.bytesWritten) },
                  { label: 'Verified', value: lastResult.verified ? 'Yes (device MD5 matches)' : 'No' },
                  ...(lastResult.skippedRegions.length
                    ? [{ label: 'Skipped', value: `${lastResult.skippedRegions.length} unchanged region(s)` }]
                    : []),
                ]}
              />
              {#if device}
                <div class="flex gap-2">
                  <Button
                    size="sm"
                    onclick={() => nav.openMonitor({ deviceId: device.id, elfPath: lastElf, autoStart: true })}
                  >
                    <Terminal />
                    Open serial monitor
                  </Button>
                </div>
              {/if}
            {/if}
          {/snippet}
        </JobPanel>
      </div>
    </div>
  </Tabs.Root>
</ViewScroll>

<ConfirmDialog
  bind:open={confirmOpen}
  title={mode === 'elf' ? 'Flash application?' : 'Write binary to flash?'}
  description="The affected flash regions will be erased and overwritten. Do not disconnect the device until the operation completes."
  confirmLabel={mode === 'elf' ? 'Flash' : 'Write'}
  tone="warning"
  onConfirm={run}
>
  {#snippet details()}
    <KeyValue
      rows={[
        { label: 'Device', value: device ? `${info?.chipName ?? 'Device'} on ${device.port.name}` : null },
        { label: 'File', value: mode === 'elf' ? (elfPath ? basename(elfPath) : null) : binPath ? basename(binPath) : null },
        ...(mode === 'elf'
          ? (preview?.segments ?? []).map((s) => ({
              label: s.label,
              value: `${formatAddress(s.offset)} · ${formatSize(s.size)}`,
              mono: true,
            }))
          : [
              {
                label: 'Region',
                value: offset?.ok && binEnd !== null ? `${formatAddress(offset.value)} – ${formatAddress(binEnd)}` : null,
                mono: true,
              },
            ]),
        ...(erasePartitions.length && mode === 'elf' ? [{ label: 'Erase first', value: erasePartitions.join(', ') }] : []),
        { label: 'Baud rate', value: connection.baud.toLocaleString('en-US') },
        { label: 'Verify', value: verify ? 'Yes' : 'No' },
      ]}
    />
  {/snippet}
</ConfirmDialog>
