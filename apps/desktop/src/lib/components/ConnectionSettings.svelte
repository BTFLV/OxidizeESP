<!--
  Per-operation connection parameters. Defaults come from the settings.
-->
<script lang="ts">
  import type { ConnectionOptions } from '$lib/bindings/ConnectionOptions';
  import type { ResetAfter } from '$lib/bindings/ResetAfter';
  import type { ResetBefore } from '$lib/bindings/ResetBefore';
  import { BAUD_RATES } from '$lib/options';
  import Field from './ui/Field.svelte';
  import Select from './ui/Select.svelte';
  import Switch from './ui/Switch.svelte';

  let {
    options = $bindable(),
    showResetAfter = true,
  }: { options: ConnectionOptions; showResetAfter?: boolean } = $props();

  const beforeItems: { value: ResetBefore; label: string; hint: string }[] = [
    { value: 'defaultReset', label: 'Automatic (DTR/RTS)', hint: 'Boards with an auto-reset circuit' },
    { value: 'usbReset', label: 'USB-Serial-JTAG', hint: 'Chips connected through their built-in USB port' },
    { value: 'noReset', label: 'None', hint: 'The chip is already in download mode' },
  ];
  const afterItems: { value: ResetAfter; label: string; hint: string }[] = [
    { value: 'hardReset', label: 'Hard reset', hint: 'Restart the application' },
    { value: 'noReset', label: 'Stay in bootloader', hint: 'Leave the ROM bootloader running' },
    { value: 'noResetNoStub', label: 'Stay in flasher stub', hint: 'Leave the RAM stub running' },
    { value: 'watchdogReset', label: 'Watchdog reset', hint: 'For USB-OTG and USB-Serial-JTAG setups' },
  ];
</script>

<div class="grid grid-cols-2 gap-3">
  <Field label="Baud rate" id="conn-baud" hint="The bootloader connects at 115200 and then switches.">
    <Select
      id="conn-baud"
      items={BAUD_RATES.map((b) => ({ value: String(b), label: b.toLocaleString('en-US') }))}
      value={String(options.baud)}
      onValueChange={(v) => (options = { ...options, baud: Number(v) })}
    />
  </Field>
  <Field label="Reset before" id="conn-before">
    <Select
      id="conn-before"
      items={beforeItems}
      value={options.resetBefore}
      onValueChange={(v) => (options = { ...options, resetBefore: v as ResetBefore })}
    />
  </Field>
  {#if showResetAfter}
    <Field label="Reset after" id="conn-after">
      <Select
        id="conn-after"
        items={afterItems}
        value={options.resetAfter}
        onValueChange={(v) => (options = { ...options, resetAfter: v as ResetAfter })}
      />
    </Field>
  {/if}
  <div class="col-span-2">
    <Switch
      label="Use flasher stub"
      description="Load espflash's RAM stub for faster transfers. Required for erasing."
      checked={options.useStub}
      onCheckedChange={(v) => (options = { ...options, useStub: v })}
    />
  </div>
</div>
