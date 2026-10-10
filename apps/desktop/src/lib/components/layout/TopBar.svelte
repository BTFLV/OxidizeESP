<script lang="ts">
  import { deviceBlocker, portDescription, statusLabel, statusTone } from '$lib/device';
  import { Plug, RefreshCw, Unplug } from '$lib/icons';
  import { activity } from '$lib/stores/activity.svelte';
  import { app } from '$lib/stores/app.svelte';
  import { devices } from '$lib/stores/devices.svelte';
  import { toasts } from '$lib/stores/toasts.svelte';
  import { cn } from '$lib/utils/cn';
  import { formatPercent } from '$lib/utils/format';
  import ActionButton from '../ui/ActionButton.svelte';
  import Badge from '../ui/Badge.svelte';
  import Button from '../ui/Button.svelte';
  import Progress from '../ui/Progress.svelte';
  import Select, { type SelectItem } from '../ui/Select.svelte';
  import Spinner from '../ui/Spinner.svelte';
  import Tooltip from '../ui/Tooltip.svelte';

  const items = $derived.by<SelectItem[]>(() => {
    const names = new Set(devices.ports.map((p) => p.name));
    const list: SelectItem[] = devices.ports.map((port) => {
      const session = devices.sessionForPort(port.name);
      const chip = session?.chipInfo?.chipName;
      return {
        value: port.name,
        label: chip ? `${port.name}  ·  ${chip}` : port.name,
        hint: portDescription(port),
      };
    });
    for (const d of devices.devices) {
      if (!names.has(d.port.name)) {
        list.push({ value: d.port.name, label: `${d.port.name}  ·  disconnected`, hint: portDescription(d.port) });
      }
    }
    return list;
  });

  const session = $derived(devices.selectedPort ? devices.sessionForPort(devices.selectedPort) : null);
  const current = $derived(activity.current);
  const busyReason = $derived(session ? deviceBlocker(session, false) : null);

  function selectPort(name: string) {
    devices.selectedPort = name;
    devices.activeId = devices.sessionForPort(name)?.id ?? null;
  }

  async function connect() {
    const port = devices.selectedPort;
    if (!port) return;
    await devices.connect(port);
    const state = devices.connectJob.state;
    if (state.phase === 'failed' && state.error) {
      toasts.show('danger', state.error.message, state.error.hint ?? undefined, 8000);
    }
  }

  async function identify() {
    if (!session) return;
    await devices.identify(session.id);
    const state = devices.connectJob.state;
    if (state.phase === 'failed' && state.error) {
      toasts.show('danger', state.error.message, state.error.hint ?? undefined, 8000);
    }
  }

  async function disconnect() {
    if (!session) return;
    const error = await devices.disconnect(session.id);
    if (error) toasts.show('danger', error.message, error.hint ?? undefined);
  }
</script>

<header data-tauri-drag-region class="flex h-12 shrink-0 items-center gap-2 border-b border-border bg-surface px-3">
  <div class="flex min-w-0 items-center gap-2">
    <Select
      {items}
      value={devices.selectedPort ?? ''}
      placeholder={devices.ports.length ? 'Select a serial port' : 'No serial ports found'}
      aria-label="Serial port"
      class="w-72"
      mono
      onValueChange={selectPort}
    />
    <Tooltip text="Refresh ports">
      {#snippet children(props)}
        <Button
          {...props}
          variant="ghost"
          size="icon"
          aria-label="Refresh serial ports"
          onclick={() => devices.refreshPorts()}
        >
          <RefreshCw class={cn(devices.refreshing && 'animate-spin')} />
        </Button>
      {/snippet}
    </Tooltip>

    {#if !session || session.status === 'disconnected'}
      <ActionButton
        variant="primary"
        loading={devices.connectJob.running}
        disabledReason={!devices.selectedPort
          ? 'Select a serial port first.'
          : session?.status === 'disconnected'
            ? 'The device is not connected to this port.'
            : null}
        onclick={connect}
      >
        <Plug />
        Connect
      </ActionButton>
    {:else}
      {#if session.status === 'unverified'}
        <ActionButton variant="primary" loading={devices.connectJob.running} disabledReason={busyReason} onclick={identify}>
          <Plug />
          Identify
        </ActionButton>
      {/if}
      <ActionButton
        variant="ghost"
        disabledReason={session.activity.type === 'operation' ? 'Wait for the current operation to finish.' : null}
        title={session.activity.type === 'monitoring'
          ? 'Stop the serial monitor and close the session'
          : 'Close the session. The device is not reset.'}
        onclick={disconnect}
      >
        <Unplug />
        Disconnect
      </ActionButton>
    {/if}

    {#if session}
      <Badge tone={statusTone(session)} dot>{statusLabel(session)}</Badge>
    {/if}
  </div>

  <div data-tauri-drag-region class="h-full min-w-4 flex-1"></div>

  {#if current}
    <div class="flex w-64 min-w-0 items-center gap-2.5" aria-live="polite">
      <Spinner class="size-3.5 shrink-0 text-accent" />
      <div class="flex min-w-0 flex-1 flex-col gap-1">
        <div class="flex items-center justify-between gap-2 text-xs">
          <span class="truncate text-fg-muted">{current.label}</span>
          {#if current.state.progress}
            <span class="shrink-0 text-fg-subtle tabular">{formatPercent(current.state.progress.overall)}</span>
          {/if}
        </div>
        <Progress value={current.state.progress?.overall ?? null} label={current.label} class="h-1" />
      </div>
    </div>
  {/if}

  {#if app.info?.simulatedHardware}
    <Badge tone="warning">Simulated hardware</Badge>
  {/if}
</header>
