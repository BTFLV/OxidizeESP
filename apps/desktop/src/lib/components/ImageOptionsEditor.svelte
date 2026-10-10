<!--
  Options for building an ESP-IDF application image from an ELF file.
  `options` always reflects valid input; `errors` lists fields that are
  currently invalid.
-->
<script lang="ts">
  import type { Chip } from '$lib/bindings/Chip';
  import type { FlashFrequency } from '$lib/bindings/FlashFrequency';
  import type { FlashMode } from '$lib/bindings/FlashMode';
  import type { FlashSize } from '$lib/bindings/FlashSize';
  import type { ImageOptions } from '$lib/bindings/ImageOptions';
  import { AUTO, FLASH_FREQUENCIES, FLASH_MODES, FLASH_SIZES } from '$lib/options';
  import { app } from '$lib/stores/app.svelte';
  import { formatHex } from '$lib/utils/format';
  import { parseNumber } from '$lib/utils/parse';
  import Field from './ui/Field.svelte';
  import FileField from './ui/FileField.svelte';
  import Input from './ui/Input.svelte';
  import Select from './ui/Select.svelte';
  import Switch from './ui/Switch.svelte';

  let {
    options = $bindable(),
    errors = $bindable([]),
    chip,
    sizeAutoLabel = 'Detected',
  }: {
    options: ImageOptions;
    errors?: string[];
    chip: Chip | null;
    sizeAutoLabel?: string;
  } = $props();

  const descriptor = $derived(app.chip(chip));

  let tableOffsetText = $state(options.partitionTableOffset !== null ? formatHex(options.partitionTableOffset) : '');
  let minRevisionText = $state(
    options.minChipRevision !== null ? `${Math.floor(options.minChipRevision / 100)}.${options.minChipRevision % 100}` : '',
  );
  let targetPartition = $state(options.targetAppPartition ?? '');

  const tableOffset = $derived.by(() => {
    if (tableOffsetText.trim() === '') return { value: null as number | null, error: null as string | null };
    const r = parseNumber(tableOffsetText);
    if (!r.ok) return { value: null, error: r.error };
    if (r.value % 0x1000 !== 0 || r.value < 0x8000) return { value: null, error: 'Must be 4 KiB aligned and at least 0x8000.' };
    return { value: r.value, error: null };
  });

  const minRevision = $derived.by(() => {
    const text = minRevisionText.trim();
    if (text === '') return { value: null as number | null, error: null as string | null };
    const m = /^(\d{1,2})\.(\d{1,2})$/.exec(text);
    if (!m) return { value: null, error: 'Use the format major.minor, for example 3.0.' };
    return { value: Number(m[1]) * 100 + Number(m[2]), error: null };
  });

  const frequencyItems = $derived([
    {
      value: AUTO,
      label: descriptor ? `Default (${descriptor.defaultFlashFrequency.replace('MHz', ' MHz')})` : 'Chip default',
    },
    ...FLASH_FREQUENCIES.filter((f) => !descriptor || descriptor.supportedFlashFrequencies.includes(f)).map((f) => ({
      value: f,
      label: f.replace('MHz', ' MHz'),
    })),
  ]);

  const mmuItems = $derived([
    { value: AUTO, label: 'From application' },
    ...(descriptor?.mmuPageSizes ?? []).map((s) => ({ value: String(s), label: `${s / 1024} KiB` })),
  ]);

  $effect(() => {
    const next: string[] = [];
    if (tableOffset.error) next.push(`Partition table offset: ${tableOffset.error}`);
    if (minRevision.error) next.push(`Minimum chip revision: ${minRevision.error}`);
    errors = next;
  });

  $effect(() => {
    const offset = tableOffset.value;
    const revision = minRevision.value;
    const label = targetPartition.trim() || null;
    if (
      offset !== options.partitionTableOffset ||
      revision !== options.minChipRevision ||
      label !== options.targetAppPartition
    ) {
      options = { ...options, partitionTableOffset: offset, minChipRevision: revision, targetAppPartition: label };
    }
  });

  $effect(() => {
    // Drop settings that the newly selected chip does not support.
    if (!descriptor) return;
    if (options.flash.frequency && !descriptor.supportedFlashFrequencies.includes(options.flash.frequency)) {
      options = { ...options, flash: { ...options.flash, frequency: null } };
    }
    if (options.mmuPageSize !== null && !descriptor.mmuPageSizes.includes(options.mmuPageSize)) {
      options = { ...options, mmuPageSize: null };
    }
  });
</script>

<div class="flex flex-col gap-4">
  <div class="grid grid-cols-3 gap-3">
    <Field label="Flash mode" id="img-mode">
      <Select
        id="img-mode"
        items={[{ value: AUTO, label: 'From bootloader' }, ...FLASH_MODES]}
        value={options.flash.mode ?? AUTO}
        onValueChange={(v) => (options = { ...options, flash: { ...options.flash, mode: v === AUTO ? null : (v as FlashMode) } })}
      />
    </Field>
    <Field label="Flash size" id="img-size">
      <Select
        id="img-size"
        items={[{ value: AUTO, label: sizeAutoLabel }, ...FLASH_SIZES.map((s) => ({ value: s, label: s.replace(/(\d+)/, '$1 ') }))]}
        value={options.flash.size ?? AUTO}
        onValueChange={(v) => (options = { ...options, flash: { ...options.flash, size: v === AUTO ? null : (v as FlashSize) } })}
      />
    </Field>
    <Field label="Flash frequency" id="img-freq">
      <Select
        id="img-freq"
        items={frequencyItems}
        value={options.flash.frequency ?? AUTO}
        onValueChange={(v) =>
          (options = { ...options, flash: { ...options.flash, frequency: v === AUTO ? null : (v as FlashFrequency) } })}
      />
    </Field>
  </div>

  <Field label="Bootloader" hint="Leave empty to use the bootloader bundled with espflash for the target chip.">
    <FileField
      path={options.bootloader}
      filters={[{ name: 'Bootloader binary', extensions: ['bin'] }]}
      dropHint="Bundled espflash bootloader"
      aria-label="Choose a bootloader binary"
      onSelect={(p) => (options = { ...options, bootloader: p })}
    />
  </Field>

  <Field label="Partition table" hint="CSV or binary. Leave empty to use espflash's default table for the chip.">
    <FileField
      path={options.partitionTable}
      filters={[{ name: 'Partition table', extensions: ['csv', 'bin'] }]}
      dropHint="Default partition table"
      aria-label="Choose a partition table"
      onSelect={(p) => (options = { ...options, partitionTable: p })}
    />
  </Field>

  <div class="grid grid-cols-2 gap-3">
    <Field label="Partition table offset" id="img-pt-offset" error={tableOffset.error} hint="Default: 0x8000">
      <Input id="img-pt-offset" bind:value={tableOffsetText} placeholder="0x8000" mono invalid={!!tableOffset.error} />
    </Field>
    <Field label="Target app partition" id="img-target" hint="Label of the app partition to place the image in.">
      <Input id="img-target" bind:value={targetPartition} placeholder="factory" mono />
    </Field>
    <Field label="Minimum chip revision" id="img-minrev" error={minRevision.error} hint="Recorded in the image header.">
      <Input id="img-minrev" bind:value={minRevisionText} placeholder="0.0" mono invalid={!!minRevision.error} />
    </Field>
    <Field label="MMU page size" id="img-mmu">
      <Select
        id="img-mmu"
        items={mmuItems}
        disabled={!descriptor || descriptor.mmuPageSizes.length === 0}
        value={options.mmuPageSize !== null ? String(options.mmuPageSize) : AUTO}
        onValueChange={(v) => (options = { ...options, mmuPageSize: v === AUTO ? null : Number(v) })}
      />
    </Field>
  </div>

  <Switch
    label="Require ESP-IDF application descriptor"
    description="espflash checks for the descriptor by default. esp-hal applications add it with esp_app_desc!()."
    checked={options.checkAppDescriptor}
    onCheckedChange={(v) => (options = { ...options, checkAppDescriptor: v })}
  />
</div>
