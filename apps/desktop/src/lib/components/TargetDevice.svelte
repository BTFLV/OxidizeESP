<!--
  Summary of the device an operation will act on, with guidance when no
  suitable device is available.
-->
<script lang="ts">
  import { deviceBlocker, deviceTitle, statusLabel, statusTone } from '$lib/device';
  import { Cpu } from '$lib/icons';
  import { devices } from '$lib/stores/devices.svelte';
  import { monitor } from '$lib/stores/monitor.svelte';
  import { flashSizeBytes } from '$lib/options';
  import { formatSize } from '$lib/utils/format';
  import Badge from './ui/Badge.svelte';
  import Button from './ui/Button.svelte';
  import Callout from './ui/Callout.svelte';

  let { requireIdentified = true }: { requireIdentified?: boolean } = $props();

  const device = $derived(devices.active);
  const blocker = $derived(deviceBlocker(device, requireIdentified));
  const flashBytes = $derived(device?.chipInfo?.flashSize ? flashSizeBytes(device.chipInfo.flashSize) : null);
</script>

<div class="flex flex-col gap-2">
  <div class="flex items-center gap-3 rounded-lg border border-border bg-surface px-3 py-2.5">
    <div class="flex size-8 shrink-0 items-center justify-center rounded-md bg-surface-3 text-fg-muted">
      <Cpu class="size-4" aria-hidden="true" />
    </div>
    {#if device}
      <div class="flex min-w-0 flex-1 flex-col">
        <span class="truncate text-[13px] font-medium text-fg">{deviceTitle(device)}</span>
        <span class="truncate text-xs text-fg-subtle">
          <span class="font-mono">{device.port.name}</span>
          {#if device.chipInfo?.macAddress}· MAC <span class="font-mono">{device.chipInfo.macAddress}</span>{/if}
          {#if flashBytes}· {formatSize(flashBytes)} flash{/if}
        </span>
      </div>
      <Badge tone={statusTone(device)} dot>{statusLabel(device)}</Badge>
    {:else}
      <div class="flex min-w-0 flex-1 flex-col">
        <span class="text-[13px] font-medium text-fg">No device</span>
        <span class="text-xs text-fg-subtle">Select a serial port in the toolbar and connect.</span>
      </div>
    {/if}
  </div>
  {#if blocker && device}
    <Callout tone="warning">
      {blocker}
      {#snippet actions()}
        {#if device.activity.type === 'monitoring' && monitor.deviceId === device.id}
          <Button size="sm" onclick={() => monitor.stop()}>Stop serial monitor</Button>
        {:else if device.status === 'unverified' && device.activity.type === 'idle'}
          <Button size="sm" loading={devices.connectJob.running} onclick={() => devices.identify(device.id)}>
            Identify device
          </Button>
        {/if}
      {/snippet}
    </Callout>
  {/if}
</div>
