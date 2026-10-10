<script lang="ts">
  import { Tabs } from 'bits-ui';
  import { api } from '$lib/api';
  import type { BackupResult } from '$lib/bindings/BackupResult';
  import type { BackupVerification } from '$lib/bindings/BackupVerification';
  import type { ChecksumResult } from '$lib/bindings/ChecksumResult';
  import type { LocalChecksum } from '$lib/bindings/LocalChecksum';
  import JobPanel from '$lib/components/JobPanel.svelte';
  import PageHeader from '$lib/components/PageHeader.svelte';
  import TargetDevice from '$lib/components/TargetDevice.svelte';
  import ActionButton from '$lib/components/ui/ActionButton.svelte';
  import Badge from '$lib/components/ui/Badge.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Callout from '$lib/components/ui/Callout.svelte';
  import CopyButton from '$lib/components/ui/CopyButton.svelte';
  import ErrorCallout from '$lib/components/ui/ErrorCallout.svelte';
  import Field from '$lib/components/ui/Field.svelte';
  import FileField from '$lib/components/ui/FileField.svelte';
  import Input from '$lib/components/ui/Input.svelte';
  import KeyValue from '$lib/components/ui/KeyValue.svelte';
  import Panel from '$lib/components/ui/Panel.svelte';
  import Segmented from '$lib/components/ui/Segmented.svelte';
  import TabBar from '$lib/components/ui/TabBar.svelte';
  import ViewScroll from '$lib/components/ViewScroll.svelte';
  import { deviceBlocker } from '$lib/device';
  import { FolderOpen, Hash, HardDriveDownload, Shield, Zap } from '$lib/icons';
  import { flashSizeBytes } from '$lib/options';
  import { app } from '$lib/stores/app.svelte';
  import { devices } from '$lib/stores/devices.svelte';
  import { JobRunner } from '$lib/stores/job.svelte';
  import { nav } from '$lib/stores/nav.svelte';
  import { settings } from '$lib/stores/settings.svelte';
  import { toasts } from '$lib/stores/toasts.svelte';
  import { type AppError, toAppError } from '$lib/utils/errors';
  import { basename, formatAddress, formatDateTime, formatHex, formatSize } from '$lib/utils/format';
  import { checkRegion } from '$lib/utils/parse';

  const device = $derived(devices.active);
  const info = $derived(device?.chipInfo ?? null);
  const blocker = $derived(deviceBlocker(device));
  const flashBytes = $derived(info?.flashSize ? flashSizeBytes(info.flashSize) : null);

  // --- Read flash ---------------------------------------------------------------
  const readJob = new JobRunner();
  let scope = $state<'full' | 'region'>('full');
  let readOffset = $state('0x0');
  let readLength = $state('');
  let outputPath = $state<string | null>(null);
  let suggestedName = $state<string | null>(null);
  let backup = $state<BackupResult | null>(null);

  const readRegion = $derived(checkRegion(readOffset, readLength, { flashSize: flashBytes }));
  const effectiveScope = $derived(scope === 'full' && !flashBytes && info ? 'region' : scope);

  $effect(() => {
    const id = device?.id;
    const region =
      effectiveScope === 'region' && readRegion.valid && readRegion.offset.ok && readRegion.length.ok
        ? { offset: readRegion.offset.value, length: readRegion.length.value }
        : null;
    if (!id || !info) {
      suggestedName = null;
      return;
    }
    api
      .suggestBackupName(id, region)
      .then((name) => (suggestedName = name))
      .catch(() => (suggestedName = null));
  });

  const readBlocker = $derived(
    readJob.running
      ? 'A backup is in progress.'
      : (blocker ??
        (effectiveScope === 'region' && !readRegion.valid ? 'Enter a valid region.' : null) ??
        (!outputPath ? 'Choose where to save the backup.' : null)),
  );

  async function startBackup() {
    if (!device || !outputPath) return;
    backup = null;
    const region =
      effectiveScope === 'region' && readRegion.offset.ok && readRegion.length.ok
        ? { offset: readRegion.offset.value, length: readRegion.length.value }
        : null;
    const path = outputPath;
    const result = await readJob.run({ label: 'Reading flash', deviceId: device.id, kind: 'readFlash' }, (l) =>
      api.readFlash({ deviceId: device.id, region, outputPath: path, connection: devices.connectionOptions() }, l),
    );
    if (result) backup = result;
  }

  async function reveal(path: string) {
    try {
      await api.revealPath(path);
    } catch (error) {
      toasts.show('danger', toAppError(error).message);
    }
  }

  // --- Checksum -------------------------------------------------------------------
  const checksumJob = new JobRunner();
  let sumOffset = $state('0x0');
  let sumLength = $state('');
  let deviceSum = $state<ChecksumResult | null>(null);
  let comparePath = $state<string | null>(null);
  let localSum = $state<LocalChecksum | null>(null);
  let localError = $state<AppError | null>(null);

  const sumRegion = $derived(checkRegion(sumOffset, sumLength, { flashSize: flashBytes }));
  const sumBlocker = $derived(
    checksumJob.running ? 'A checksum is being calculated.' : (blocker ?? (!sumRegion.valid ? 'Enter a valid region.' : null)),
  );

  async function calculate() {
    if (!device || !sumRegion.offset.ok || !sumRegion.length.ok) return;
    const region = { offset: sumRegion.offset.value, length: sumRegion.length.value };
    deviceSum = null;
    const result = await checksumJob.run({ label: 'Calculating MD5', deviceId: device.id, kind: 'checksumMd5' }, (l) =>
      api.checksumMd5({ deviceId: device.id, region, connection: devices.connectionOptions() }, l),
    );
    if (result) deviceSum = result;
    await computeLocal();
  }

  async function computeLocal() {
    localSum = null;
    localError = null;
    if (!comparePath) return;
    const length = sumRegion.length.ok ? sumRegion.length.value : null;
    try {
      localSum = await api.fileMd5(comparePath, 0, length);
    } catch (error) {
      localError = toAppError(error);
    }
  }

  async function pickCompare(path: string | null) {
    comparePath = path;
    localSum = null;
    localError = null;
    if (!path) return;
    try {
      const file = await api.inspectFile(path);
      if (sumLength.trim() === '') sumLength = formatHex(file.size);
      if (file.backup && sumOffset.trim() === '0x0') sumOffset = formatHex(file.backup.flashOffset);
      await computeLocal();
    } catch (error) {
      localError = toAppError(error);
    }
  }

  const sumsMatch = $derived(deviceSum && localSum ? deviceSum.md5 === localSum.md5 : null);

  // --- Verify a backup file -----------------------------------------------------------
  let verifyPath = $state<string | null>(null);
  let verification = $state<BackupVerification | null>(null);
  let verifyError = $state<AppError | null>(null);

  async function verifyBackup(path: string | null) {
    verifyPath = path;
    verification = null;
    verifyError = null;
    if (!path) return;
    try {
      verification = await api.verifyBackup(path);
    } catch (error) {
      verifyError = toAppError(error);
    }
  }
</script>

<ViewScroll>
  <PageHeader title="Backup" description="Read flash contents to a file and compare checksums." />

  <Tabs.Root value="read" class="flex flex-col gap-5">
    <TabBar
      items={[
        { value: 'read', label: 'Read flash', icon: HardDriveDownload },
        { value: 'checksum', label: 'Checksum (MD5)', icon: Hash },
        { value: 'verify', label: 'Verify backup', icon: Shield },
      ]}
      class="self-start"
    />

    <Tabs.Content value="read">
      <div class="grid grid-cols-1 items-start gap-5 xl:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]">
        <div class="flex min-w-0 flex-col gap-5">
          <Panel title="What to read">
            <div class="flex flex-col gap-4">
              <Segmented
                label="Backup scope"
                options={[
                  { value: 'full', label: 'Entire flash', disabled: !!info && !flashBytes },
                  { value: 'region', label: 'Region' },
                ]}
                bind:value={scope}
                class="self-start"
              />
              {#if effectiveScope === 'full'}
                <p class="text-[13px] text-fg-muted">
                  {#if flashBytes}
                    Reads all {formatSize(flashBytes)} ({formatAddress(0)} – {formatAddress(flashBytes)}) as detected on the device.
                  {:else if info}
                    The flash size could not be detected. Specify a region instead.
                  {:else}
                    Identify the device to detect its flash size.
                  {/if}
                </p>
              {:else}
                <div class="grid grid-cols-2 gap-3">
                  <Field label="Start address" id="read-offset" error={readRegion.offset.ok ? null : readRegion.offset.error}>
                    <Input id="read-offset" bind:value={readOffset} mono invalid={!readRegion.offset.ok} />
                  </Field>
                  <Field
                    label="Length"
                    id="read-length"
                    error={readRegion.length.ok ? readRegion.region : readLength.trim() ? readRegion.length.error : null}
                    hint="Hexadecimal, decimal, or with K/M suffix."
                  >
                    <Input id="read-length" bind:value={readLength} placeholder="0x10000" mono invalid={!!readLength.trim() && !readRegion.valid} />
                  </Field>
                </div>
              {/if}
            </div>
          </Panel>

          <Panel title="Destination">
            <div class="flex flex-col gap-3">
              <FileField
                mode="save"
                bind:path={outputPath}
                filters={[{ name: 'Binary', extensions: ['bin'] }]}
                defaultPath={settings.value.backupDir}
                {suggestedName}
                placeholder="Choose a file name…"
                aria-label="Choose where to save the backup"
              />
              <p class="text-xs text-fg-subtle">
                Data is written to a temporary file and only saved under this name after the complete region was read.
                A metadata file (<span class="font-mono">.oxidizeesp.json</span>) with the SHA-256 checksum is saved next to it.
              </p>
            </div>
          </Panel>

          <Callout tone="neutral" title="What a backup contains">
            A backup is a copy of the SPI flash contents only. It does not include eFuses or other chip configuration. With
            flash encryption or Secure Boot enabled, the data is tied to this specific device.
          </Callout>
        </div>

        <div class="flex min-w-0 flex-col gap-4">
          <TargetDevice />
          <ActionButton variant="primary" class="w-full" disabledReason={readBlocker} onclick={startBackup}>
            <HardDriveDownload />
            Read flash
          </ActionButton>
          <JobPanel runner={readJob}>
            {#snippet success()}
              {#if backup}
                {@const meta = backup.metadata}
                <div class="flex flex-col gap-3">
                  <KeyValue
                    rows={[
                      { label: 'File', value: basename(backup.path) },
                      { label: 'Region', value: `${formatAddress(meta.flashOffset)} – ${formatAddress(meta.flashOffset + meta.length)}`, mono: true },
                      { label: 'Size', value: formatSize(meta.length) },
                      { label: 'Chip', value: app.chip(meta.chip)?.name ?? meta.chip },
                      { label: 'Created', value: formatDateTime(meta.createdAt) },
                    ]}
                  />
                  <div class="flex items-center gap-1.5 rounded-md border border-border bg-surface-2 px-2.5 py-1.5">
                    <span class="text-2xs font-medium text-fg-subtle">SHA-256</span>
                    <span class="min-w-0 flex-1 truncate font-mono text-2xs text-fg">{meta.sha256}</span>
                    <CopyButton text={meta.sha256} label="Copy SHA-256" />
                  </div>
                  <div class="flex flex-wrap gap-2">
                    <Button size="sm" onclick={() => reveal(backup!.path)}><FolderOpen /> Show in folder</Button>
                    <Button
                      size="sm"
                      onclick={() => nav.openFlash({ mode: 'bin', path: backup!.path, offset: meta.flashOffset })}
                    >
                      <Zap /> Use for flashing
                    </Button>
                  </div>
                </div>
              {/if}
            {/snippet}
          </JobPanel>
        </div>
      </div>
    </Tabs.Content>

    <Tabs.Content value="checksum">
      <div class="grid grid-cols-1 items-start gap-5 xl:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]">
        <div class="flex min-w-0 flex-col gap-5">
          <Panel
            title="Device checksum"
            description="MD5 calculated on the chip by the bootloader or flasher stub, without transferring the data."
          >
            <div class="grid grid-cols-2 gap-3">
              <Field label="Start address" id="sum-offset" error={sumRegion.offset.ok ? null : sumRegion.offset.error}>
                <Input id="sum-offset" bind:value={sumOffset} mono invalid={!sumRegion.offset.ok} />
              </Field>
              <Field
                label="Length"
                id="sum-length"
                error={sumRegion.length.ok ? sumRegion.region : sumLength.trim() ? sumRegion.length.error : null}
              >
                <Input id="sum-length" bind:value={sumLength} placeholder="0x10000" mono invalid={!!sumLength.trim() && !sumRegion.valid} />
              </Field>
            </div>
          </Panel>

          <Panel title="Compare with a file" description="Optional. Calculates the MD5 of the file locally for comparison.">
            <div class="flex flex-col gap-3">
              <FileField path={comparePath} aria-label="Choose a file to compare" onSelect={pickCompare} />
              {#if localError}<ErrorCallout error={localError} />{/if}
              <p class="text-xs text-fg-subtle">
                The first <em>length</em> bytes of the file are compared with the device region. Backups created by OxidizeESP
                also carry a SHA-256 checksum, which is calculated locally and is a different algorithm.
              </p>
            </div>
          </Panel>
        </div>

        <div class="flex min-w-0 flex-col gap-4">
          <TargetDevice />
          <ActionButton variant="primary" class="w-full" disabledReason={sumBlocker} onclick={calculate}>
            <Hash />
            Calculate device MD5
          </ActionButton>
          <JobPanel runner={checksumJob} />
          {#if deviceSum || localSum}
            <Panel title="Result">
              <div class="flex flex-col gap-3">
                {#if deviceSum}
                  <div class="flex flex-col gap-1">
                    <span class="text-xs text-fg-subtle">
                      Device MD5 · {formatAddress(deviceSum.region.offset)} + {formatHex(deviceSum.region.length)}
                    </span>
                    <div class="flex items-center gap-1.5">
                      <span class="min-w-0 flex-1 truncate font-mono text-xs text-fg">{deviceSum.md5}</span>
                      <CopyButton text={deviceSum.md5} label="Copy device MD5" />
                    </div>
                  </div>
                {/if}
                {#if localSum}
                  <div class="flex flex-col gap-1">
                    <span class="text-xs text-fg-subtle">File MD5 · {comparePath ? basename(comparePath) : ''}</span>
                    <div class="flex items-center gap-1.5">
                      <span class="min-w-0 flex-1 truncate font-mono text-xs text-fg">{localSum.md5}</span>
                      <CopyButton text={localSum.md5} label="Copy file MD5" />
                    </div>
                  </div>
                {/if}
                {#if sumsMatch !== null}
                  <Badge tone={sumsMatch ? 'success' : 'danger'} dot class="self-start">
                    {sumsMatch ? 'Checksums match' : 'Checksums differ'}
                  </Badge>
                {/if}
              </div>
            </Panel>
          {/if}
        </div>
      </div>
    </Tabs.Content>

    <Tabs.Content value="verify">
      <div class="flex max-w-3xl flex-col gap-5">
        <Panel title="Verify a backup file" description="Recalculates the SHA-256 checksum and compares it with the metadata file.">
          <div class="flex flex-col gap-4">
            <FileField
              path={verifyPath}
              filters={[{ name: 'Backup', extensions: ['bin', '*'] }]}
              defaultPath={settings.value.backupDir}
              aria-label="Choose a backup to verify"
              onSelect={verifyBackup}
            />
            {#if verifyError}<ErrorCallout error={verifyError} />{/if}
            {#if verification}
              {#if verification.matches === true}
                <Callout tone="success" title="The backup is intact">The file matches the checksum and size recorded when it was created.</Callout>
              {:else if verification.matches === false}
                <Callout tone="danger" title="The backup does not match its metadata">
                  The file was modified or damaged after it was created.
                </Callout>
              {:else}
                <Callout tone="neutral" title="No metadata">
                  This file has no OxidizeESP metadata, so it cannot be verified. Its SHA-256 checksum is shown below.
                </Callout>
              {/if}
              <KeyValue
                rows={[
                  { label: 'SHA-256', value: verification.actualSha256, mono: true },
                  { label: 'Size', value: formatSize(verification.actualSize) },
                  ...(verification.metadata
                    ? [
                        { label: 'Recorded SHA-256', value: verification.metadata.sha256, mono: true },
                        { label: 'Chip', value: app.chip(verification.metadata.chip)?.name ?? verification.metadata.chip },
                        { label: 'MAC address', value: verification.metadata.macAddress, mono: true },
                        {
                          label: 'Region',
                          value: `${formatAddress(verification.metadata.flashOffset)} – ${formatAddress(verification.metadata.flashOffset + verification.metadata.length)}`,
                          mono: true,
                        },
                        { label: 'Created', value: formatDateTime(verification.metadata.createdAt) },
                        { label: 'Created with', value: `${verification.metadata.application} ${verification.metadata.applicationVersion}` },
                      ]
                    : []),
                ]}
              />
              {#if verification.metadata && verifyPath}
                <div>
                  <Button
                    size="sm"
                    onclick={() => nav.openFlash({ mode: 'bin', path: verifyPath!, offset: verification!.metadata!.flashOffset })}
                  >
                    <Zap /> Use for flashing
                  </Button>
                </div>
              {/if}
            {/if}
          </div>
        </Panel>
      </div>
    </Tabs.Content>
  </Tabs.Root>
</ViewScroll>
