<script lang="ts">
  import { deviceTitle, statusLabel, statusTone } from '$lib/device';
  import { activity } from '$lib/stores/activity.svelte';
  import { app } from '$lib/stores/app.svelte';
  import { devices } from '$lib/stores/devices.svelte';
  import { monitor } from '$lib/stores/monitor.svelte';
  import { cn } from '$lib/utils/cn';
  import { formatPercent, formatSize } from '$lib/utils/format';

  const device = $derived(devices.active);
  const tone = $derived(statusTone(device));
  const dot: Record<string, string> = {
    neutral: 'bg-fg-subtle',
    success: 'bg-success',
    warning: 'bg-warning',
    danger: 'bg-danger',
    info: 'bg-info',
    accent: 'bg-accent',
  };
</script>

<footer class="flex h-6 shrink-0 items-center gap-4 border-t border-border bg-surface px-3 text-2xs text-fg-subtle">
  <div class="flex min-w-0 items-center gap-1.5">
    <span class={cn('size-1.5 shrink-0 rounded-full', dot[tone])} aria-hidden="true"></span>
    {#if device}
      <span class="truncate text-fg-muted">
        {deviceTitle(device)}
        {#if device.chipInfo?.flashSize}· {device.chipInfo.flashSize} flash{/if}
        · <span class="font-mono">{device.port.name}</span> · {statusLabel(device)}
      </span>
    {:else}
      <span>No device selected</span>
    {/if}
  </div>
  {#if activity.runners.length > 0}
    <div class="flex min-w-0 items-center gap-1.5 truncate">
      {#each activity.runners as runner, i (i)}
        <span class="truncate">
          {runner.label}{#if runner.state.progress}&nbsp;{formatPercent(runner.state.progress.overall)}{/if}
        </span>
      {/each}
    </div>
  {/if}
  {#if monitor.status === 'running'}
    <span class="truncate">Monitor: {monitor.port} · {monitor.received.toLocaleString()} lines</span>
  {:else if monitor.status === 'waiting'}
    <span class="truncate text-warning">Monitor waiting for the device to reconnect</span>
  {/if}
  <div class="flex-1"></div>
  {#if devices.ports.length}
    <span>{devices.ports.length} port{devices.ports.length === 1 ? '' : 's'}</span>
  {/if}
  {#if app.info}
    <span>espflash {app.info.espflashVersion}</span>
  {/if}
</footer>
