<script lang="ts">
  import { save as saveDialog } from '@tauri-apps/plugin-dialog';
  import { api } from '$lib/api';
  import type { LineEnding } from '$lib/bindings/LineEnding';
  import type { MonitorLine } from '$lib/bindings/MonitorLine';
  import LogView from '$lib/components/LogView.svelte';
  import ActionButton from '$lib/components/ui/ActionButton.svelte';
  import Badge from '$lib/components/ui/Badge.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import ErrorCallout from '$lib/components/ui/ErrorCallout.svelte';
  import FileField from '$lib/components/ui/FileField.svelte';
  import Input from '$lib/components/ui/Input.svelte';
  import Select from '$lib/components/ui/Select.svelte';
  import Switch from '$lib/components/ui/Switch.svelte';
  import Tooltip from '$lib/components/ui/Tooltip.svelte';
  import {
    ChevronDown,
    ChevronUp,
    Clock,
    Copy,
    Funnel,
    Pause,
    Play,
    RotateCcw,
    Save,
    ScrollText,
    Search,
    Send,
    Square,
    Trash,
    X,
  } from '$lib/icons';
  import { MONITOR_BAUD_RATES } from '$lib/options';
  import { devices } from '$lib/stores/devices.svelte';
  import { monitor } from '$lib/stores/monitor.svelte';
  import { nav } from '$lib/stores/nav.svelte';
  import { settings } from '$lib/stores/settings.svelte';
  import { toasts } from '$lib/stores/toasts.svelte';
  import { stripAnsi } from '$lib/utils/ansi';
  import { cn } from '$lib/utils/cn';
  import { toAppError } from '$lib/utils/errors';
  import { basename, formatTime } from '$lib/utils/format';

  let { active }: { active: boolean } = $props();

  let baud = $state(String(settings.value.monitor.baud));
  let elfPath = $state<string | null>(null);
  let logPath = $state<string | null>(null);
  let filter = $state('');
  let search = $state('');
  let matchIndex = $state(0);
  let sendText = $state('');
  let lineEnding = $state<LineEnding>(settings.value.monitor.lineEnding);
  let showTimestamps = $state(settings.value.monitor.showTimestamps);
  let autoscroll = $state(settings.value.monitor.autoscroll);
  let resetOnStart = $state(settings.value.monitor.resetOnStart);
  let receivedAtPause = $state(0);
  // The buffer is not reactive itself; `version` changes whenever it does.
  const bufferLength = $derived((monitor.version, monitor.buffer.length));

  function togglePause() {
    if (!monitor.paused) receivedAtPause = monitor.received;
    monitor.paused = !monitor.paused;
  }

  $effect(() => {
    if (!settings.loaded) return;
    const m = settings.value.monitor;
    baud = String(m.baud);
    lineEnding = m.lineEnding;
    showTimestamps = m.showTimestamps;
    autoscroll = m.autoscroll;
    resetOnStart = m.resetOnStart;
  });

  // The device the monitor will use: the running session, otherwise the
  // device or port selected in the toolbar.
  const targetPort = $derived(devices.selectedPort);
  const targetDevice = $derived(
    monitor.deviceId && monitor.status !== 'stopped'
      ? (devices.devices.find((d) => d.id === monitor.deviceId) ?? null)
      : targetPort
        ? devices.sessionForPort(targetPort)
        : null,
  );

  const startBlocker = $derived.by(() => {
    if (!targetPort && !targetDevice) return 'Select a serial port in the toolbar.';
    const d = targetDevice;
    if (d?.status === 'disconnected') return 'The device is disconnected.';
    if (d?.activity.type === 'operation') return 'The device is busy with another operation.';
    if (d?.activity.type === 'monitoring' && monitor.deviceId !== d.id) return 'Another monitor is using this device.';
    return null;
  });

  async function start() {
    let deviceId = targetDevice?.id ?? null;
    if (!deviceId && targetPort) {
      try {
        deviceId = (await devices.open(targetPort)).id;
      } catch (error) {
        toasts.show('danger', toAppError(error).message);
        return;
      }
    }
    if (!deviceId) return;
    monitor.clear();
    const error = await monitor.start({
      deviceId,
      baud: Number(baud),
      elfPath: settings.value.monitor.decodeAddresses ? elfPath : null,
      logPath,
      resetOnStart,
    });
    if (error) toasts.show('danger', error.message, error.hint ?? undefined);
  }

  // Requests from other views (for example "open monitor after flashing").
  $effect(() => {
    const intent = nav.monitorIntent;
    if (!intent) return;
    nav.monitorIntent = null;
    if (intent.elfPath) elfPath = intent.elfPath;
    const device = devices.devices.find((d) => d.id === intent.deviceId);
    if (device) {
      devices.selectedPort = device.port.name;
      devices.activeId = device.id;
    }
    if (intent.autoStart && !monitor.active) void start();
  });

  // Rows shown in the view. While paused, the snapshot is not updated.
  let rows = $state.raw<MonitorLine[]>([]);
  $effect(() => {
    void monitor.version;
    const needle = filter.trim().toLowerCase();
    if (monitor.paused || !active) return;
    const all = monitor.buffer.toArray();
    rows = needle ? all.filter((l) => stripAnsi(l.text).toLowerCase().includes(needle)) : all;
  });

  const matches = $derived.by(() => {
    const needle = search.trim().toLowerCase();
    if (!needle) return [] as number[];
    const out: number[] = [];
    rows.forEach((l, i) => {
      if (stripAnsi(l.text).toLowerCase().includes(needle)) out.push(i);
    });
    return out;
  });
  const activeRow = $derived(matches.length ? (matches[Math.min(matchIndex, matches.length - 1)] ?? null) : null);

  $effect(() => {
    void search;
    matchIndex = 0;
  });

  function step(delta: number) {
    if (!matches.length) return;
    matchIndex = (matchIndex + delta + matches.length) % matches.length;
  }

  function linesToText(lines: MonitorLine[]): string {
    return lines
      .map((l) => {
        const prefix = l.kind === 'sent' ? '> ' : l.kind === 'decoded' ? '  ' : '';
        const text = `${prefix}${stripAnsi(l.text)}`;
        return showTimestamps ? `${formatTime(l.timestamp)}  ${text}` : text;
      })
      .join('\n');
  }

  async function copyAll() {
    try {
      await navigator.clipboard.writeText(linesToText(rows));
      toasts.show('success', `Copied ${rows.length.toLocaleString('en-US')} lines`, undefined, 2500);
    } catch {
      toasts.show('danger', 'The clipboard is not available.');
    }
  }

  async function saveLog() {
    const path = await saveDialog({
      filters: [{ name: 'Log file', extensions: ['log', 'txt'] }],
      defaultPath: `serial-${new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19)}.log`,
    });
    if (typeof path !== 'string') return;
    try {
      await api.saveTextFile(path, linesToText(monitor.buffer.toArray()));
      toasts.show('success', `Saved ${basename(path)}`, undefined, 3000);
    } catch (error) {
      toasts.show('danger', toAppError(error).message);
    }
  }

  async function send() {
    if (!sendText && lineEnding === 'none') return;
    const error = await monitor.send(sendText, lineEnding);
    if (error) toasts.show('danger', error.message);
    else sendText = '';
  }

  async function reset() {
    const error = await monitor.reset();
    if (error) toasts.show('danger', error.message);
  }

  const statusTone = $derived(
    monitor.status === 'running' ? 'success' : monitor.status === 'waiting' ? 'warning' : monitor.status === 'stopped' ? 'neutral' : 'info',
  );
  const statusText = $derived(
    {
      running: 'Running',
      starting: 'Starting',
      stopping: 'Stopping',
      stopped: 'Stopped',
      waiting: 'Waiting for device',
    }[monitor.status],
  );
</script>

<div class="flex h-full flex-col">
  <div class="flex flex-col gap-2 border-b border-border bg-surface px-4 py-2.5">
    <div class="flex flex-wrap items-center gap-2">
      {#if monitor.active || monitor.status === 'waiting'}
        <Button variant="secondary" onclick={() => monitor.stop()} loading={monitor.status === 'stopping'}>
          <Square /> Stop
        </Button>
      {:else}
        <ActionButton variant="primary" disabledReason={startBlocker} onclick={start}>
          <Play /> Start
        </ActionButton>
      {/if}
      <ActionButton
        disabledReason={monitor.status !== 'running' ? 'Start the monitor first.' : null}
        title="Reset the chip (DTR/RTS)"
        onclick={reset}
      >
        <RotateCcw /> Reset
      </ActionButton>

      <div class="mx-1 h-5 w-px bg-border"></div>

      <Select
        items={MONITOR_BAUD_RATES.map((b) => ({ value: String(b), label: `${b.toLocaleString('en-US')} baud` }))}
        bind:value={baud}
        disabled={monitor.active}
        aria-label="Monitor baud rate"
        class="w-40"
      />
      <div class="w-64">
        <FileField
          path={elfPath}
          filters={[{ name: 'ELF executable', extensions: ['elf', 'out', '*'] }]}
          dropHint="ELF for decoding (optional)"
          compact
          disabled={monitor.active}
          aria-label="Choose an ELF file for address decoding"
          onSelect={(p) => (elfPath = p)}
        />
      </div>
      <div class="w-56">
        <FileField
          mode="save"
          path={logPath}
          filters={[{ name: 'Log file', extensions: ['log', 'txt'] }]}
          placeholder="Record to file (optional)"
          compact
          suggestedName="serial.log"
          disabled={monitor.active}
          aria-label="Choose a file to record output to"
          onSelect={(p) => (logPath = p)}
        />
      </div>
      <Switch label="Reset on start" bind:checked={resetOnStart} disabled={monitor.active} class="items-center gap-2" />

      <div class="flex-1"></div>
      <Badge tone={statusTone} dot>{statusText}</Badge>
      {#if monitor.status === 'running'}
        <span class="font-mono text-2xs text-fg-subtle">{monitor.port}</span>
        {#if monitor.symbolsLoaded}<Badge tone="info">Decoding addresses</Badge>{/if}
      {/if}
    </div>

    <div class="flex flex-wrap items-center gap-2">
      <div class="relative w-56">
        <Funnel class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-fg-subtle" aria-hidden="true" />
        <Input bind:value={filter} placeholder="Filter lines" class="pl-8" aria-label="Filter lines" />
      </div>
      <div class="relative w-56">
        <Search class="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-fg-subtle" aria-hidden="true" />
        <Input
          bind:value={search}
          placeholder="Search"
          class="pr-16 pl-8"
          aria-label="Search output"
          onkeydown={(e: KeyboardEvent) => {
            if (e.key === 'Enter') step(e.shiftKey ? -1 : 1);
          }}
        />
        {#if search}
          <span class="absolute top-1/2 right-2.5 -translate-y-1/2 text-2xs text-fg-subtle tabular">
            {matches.length ? `${Math.min(matchIndex, matches.length - 1) + 1}/${matches.length}` : '0/0'}
          </span>
        {/if}
      </div>
      <Button variant="ghost" size="icon-sm" aria-label="Previous match" disabled={!matches.length} onclick={() => step(-1)}><ChevronUp /></Button>
      <Button variant="ghost" size="icon-sm" aria-label="Next match" disabled={!matches.length} onclick={() => step(1)}><ChevronDown /></Button>
      {#if filter || search}
        <Button variant="ghost" size="sm" onclick={() => ((filter = ''), (search = ''))}><X /> Clear</Button>
      {/if}

      <div class="flex-1"></div>

      <Tooltip text={monitor.paused ? 'Resume display' : 'Pause display (data is still received)'}>
        {#snippet children(props)}
          <Button
            {...props}
            variant={monitor.paused ? 'primary' : 'ghost'}
            size="sm"
            aria-pressed={monitor.paused}
            onclick={togglePause}
          >
            {#if monitor.paused}<Play /> Resume{:else}<Pause /> Pause{/if}
          </Button>
        {/snippet}
      </Tooltip>
      <Button variant="ghost" size="sm" aria-pressed={showTimestamps} class={cn(showTimestamps && 'text-fg')} onclick={() => (showTimestamps = !showTimestamps)}>
        <Clock /> <span class="max-xl:sr-only">Timestamps</span>
      </Button>
      <Button variant="ghost" size="sm" aria-pressed={autoscroll} class={cn(autoscroll && 'text-fg')} onclick={() => (autoscroll = !autoscroll)}>
        <ScrollText /> <span class="max-xl:sr-only">Autoscroll</span>
      </Button>
      <div class="mx-1 h-5 w-px bg-border"></div>
      <Tooltip text="Copy the displayed lines">
        {#snippet children(props)}
          <Button {...props} variant="ghost" size="icon-sm" aria-label="Copy displayed lines" disabled={!rows.length} onclick={copyAll}><Copy /></Button>
        {/snippet}
      </Tooltip>
      <Tooltip text="Save the buffer to a file">
        {#snippet children(props)}
          <Button {...props} variant="ghost" size="icon-sm" aria-label="Save log" disabled={bufferLength === 0} onclick={saveLog}><Save /></Button>
        {/snippet}
      </Tooltip>
      <Tooltip text="Clear the display">
        {#snippet children(props)}
          <Button {...props} variant="ghost" size="icon-sm" aria-label="Clear output" onclick={() => monitor.clear()}><Trash /></Button>
        {/snippet}
      </Tooltip>
    </div>
  </div>

  {#if monitor.error && monitor.status !== 'running'}
    <div class="border-b border-border px-4 py-2">
      <ErrorCallout error={monitor.error}>
        {#snippet actions()}
          {#if monitor.status === 'waiting'}
            <span class="text-xs text-fg-muted">The monitor restarts automatically when the device reappears.</span>
          {/if}
        {/snippet}
      </ErrorCallout>
    </div>
  {/if}

  <div class="relative min-h-0 flex-1">
    <LogView {rows} {showTimestamps} {autoscroll} search={search.trim()} {activeRow} />
    {#if rows.length === 0}
      <div class="pointer-events-none absolute inset-0 flex items-center justify-center text-[13px] text-fg-subtle">
        {#if filter && bufferLength > 0}
          No lines match the filter.
        {:else if monitor.status === 'running'}
          Waiting for output…
        {:else}
          Start the monitor to see serial output.
        {/if}
      </div>
    {/if}
    {#if monitor.paused}
      <div class="absolute top-2 left-1/2 -translate-x-1/2 rounded-md border border-warning/40 bg-warning-soft px-2.5 py-1 text-xs text-warning">
        Display paused · {Math.max(0, monitor.received - receivedAtPause).toLocaleString('en-US')} new lines received
      </div>
    {/if}
  </div>

  <form
    class="flex items-center gap-2 border-t border-border bg-surface px-4 py-2"
    onsubmit={(e) => {
      e.preventDefault();
      void send();
    }}
  >
    <Input
      bind:value={sendText}
      placeholder={monitor.status === 'running' ? 'Send to device' : 'Start the monitor to send data'}
      disabled={monitor.status !== 'running'}
      mono
      aria-label="Text to send"
    />
    <Select
      items={[
        { value: 'lf', label: 'LF' },
        { value: 'crLf', label: 'CR LF' },
        { value: 'cr', label: 'CR' },
        { value: 'none', label: 'No line ending' },
      ]}
      value={lineEnding}
      onValueChange={(v) => (lineEnding = v as LineEnding)}
      aria-label="Line ending"
      class="w-36"
    />
    <Button type="submit" disabled={monitor.status !== 'running'}><Send /> Send</Button>
    <span class="ml-2 shrink-0 text-2xs text-fg-subtle tabular">
      {bufferLength.toLocaleString('en-US')} / {monitor.buffer.capacity.toLocaleString('en-US')} lines
    </span>
  </form>
</div>
