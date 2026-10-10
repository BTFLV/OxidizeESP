<script lang="ts">
  import { api } from '$lib/api';
  import type { SerialPortDescriptor } from '$lib/bindings/SerialPortDescriptor';
  import JobPanel from '$lib/components/JobPanel.svelte';
  import PageHeader from '$lib/components/PageHeader.svelte';
  import ActionButton from '$lib/components/ui/ActionButton.svelte';
  import Badge from '$lib/components/ui/Badge.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Callout from '$lib/components/ui/Callout.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import ErrorCallout from '$lib/components/ui/ErrorCallout.svelte';
  import KeyValue, { type KeyValueRow } from '$lib/components/ui/KeyValue.svelte';
  import Panel from '$lib/components/ui/Panel.svelte';
  import ViewScroll from '$lib/components/ViewScroll.svelte';
  import { deviceBlocker, deviceTitle, portDescription, statusLabel, statusTone } from '$lib/device';
  import { Cpu, Plug, Power, RefreshCw, RotateCcw, Unplug, Usb } from '$lib/icons';
  import { devices } from '$lib/stores/devices.svelte';
  import { JobRunner } from '$lib/stores/job.svelte';
  import { cn } from '$lib/utils/cn';
  import { formatDateTime, formatVidPid } from '$lib/utils/format';

  const job = new JobRunner();

  const selectedPort = $derived(devices.ports.find((p) => p.name === devices.selectedPort) ?? null);
  const session = $derived(devices.selectedPort ? devices.sessionForPort(devices.selectedPort) : null);
  const info = $derived(session?.chipInfo ?? null);
  const port = $derived(session?.port ?? selectedPort);
  const operationBlocker = $derived(deviceBlocker(session, false));

  function hintBadge(p: SerialPortDescriptor): { label: string; tone: 'accent' | 'neutral' } | null {
    if (p.hint === 'espressifUsb') return { label: 'Espressif USB', tone: 'accent' };
    if (p.hint === 'usbUartBridge') return { label: 'USB-UART bridge', tone: 'neutral' };
    return null;
  }

  function select(p: SerialPortDescriptor) {
    devices.selectedPort = p.name;
    devices.activeId = devices.sessionForPort(p.name)?.id ?? null;
  }

  const chipRows = $derived<KeyValueRow[]>(
    info
      ? [
          { label: 'Chip', value: info.chipName },
          { label: 'Revision', value: info.revision ? `v${info.revision}` : null },
          { label: 'Crystal', value: info.crystal.replace('MHz', ' MHz') },
          {
            label: 'Flash size',
            value: info.flashSize ? info.flashSize.replace(/(\d+)/, '$1 ') : 'Not detected',
          },
          { label: 'MAC address', value: info.macAddress, mono: true },
          { label: 'Features', value: info.features.join(', ') },
          { label: 'Flasher stub', value: info.stubLoaded ? 'Loaded' : 'Not used' },
          { label: 'Identified', value: session?.verifiedAt ? formatDateTime(session.verifiedAt) : null },
        ]
      : [],
  );

  const securityRows = $derived<KeyValueRow[]>(
    info?.security
      ? [
          { label: 'Secure Boot', value: info.security.secureBoot ? 'Enabled' : 'Disabled' },
          { label: 'Flash encryption', value: info.security.flashEncryption ? 'Enabled' : 'Disabled' },
          { label: 'Secure Download Mode', value: info.security.secureDownloadMode ? 'Enabled' : 'Disabled' },
          { label: 'JTAG', value: info.security.jtagDisabled ? 'Disabled by eFuse' : 'Not disabled' },
        ]
      : [],
  );

  const portRows = $derived<KeyValueRow[]>(
    port
      ? [
          { label: 'Port', value: port.name, mono: true },
          { label: 'Interface', value: port.kind === 'usb' ? 'USB' : port.kind === 'pci' ? 'PCI' : 'Unknown' },
          { label: 'VID:PID', value: port.usb ? formatVidPid(port.usb.vid, port.usb.pid) : null, mono: true },
          { label: 'Manufacturer', value: port.usb?.manufacturer },
          { label: 'Product', value: port.usb?.product },
          { label: 'Serial number', value: port.usb?.serialNumber, mono: true },
          { label: 'Bridge', value: port.bridge },
        ]
      : [],
  );

  async function reset() {
    if (!session) return;
    await job.run({ label: 'Resetting device', deviceId: session.id, kind: 'reset' }, (l) => api.resetDevice(session.id, l));
  }

  async function holdInReset() {
    if (!session) return;
    await job.run({ label: 'Holding in reset', deviceId: session.id, kind: 'holdInReset' }, (l) =>
      api.holdInReset(session.id, devices.connectionOptions(), l),
    );
  }
</script>

<ViewScroll>
  <PageHeader title="Dashboard" description="Serial ports on this computer and the devices you are working with." />

  <div class="grid grid-cols-1 items-start gap-5 lg:grid-cols-[minmax(0,5fr)_minmax(0,7fr)]">
    <Panel title="Serial ports" description="Listed from the operating system; ports are not opened." bodyClass="p-0">
      {#snippet actions()}
        <Button variant="ghost" size="sm" onclick={() => devices.refreshPorts()}>
          <RefreshCw class={cn(devices.refreshing && 'animate-spin')} />
          Refresh
        </Button>
      {/snippet}
      {#if devices.portsError}
        <div class="p-3"><ErrorCallout error={devices.portsError} /></div>
      {/if}
      {#if devices.ports.length === 0}
        <EmptyState
          icon={Usb}
          title="No serial ports found"
          description="Connect an ESP development board with a data-capable USB cable. Some boards need a USB-to-UART driver (CP210x, CH34x or FTDI)."
        />
      {:else}
        <ul class="flex flex-col divide-y divide-border" aria-label="Serial ports">
          {#each devices.ports as p (p.name)}
            {@const s = devices.sessionForPort(p.name)}
            {@const hint = hintBadge(p)}
            {@const selected = devices.selectedPort === p.name}
            <li>
              <button
                type="button"
                onclick={() => select(p)}
                aria-pressed={selected}
                class={cn(
                  'flex w-full items-center gap-3 px-4 py-2.5 text-left transition-colors outline-none focus-visible:bg-surface-2',
                  selected ? 'bg-surface-2' : 'hover:bg-surface-2/60',
                )}
              >
                <span class={cn('h-8 w-0.5 shrink-0 rounded-full', selected ? 'bg-accent' : 'bg-transparent')}></span>
                <div class="flex min-w-0 flex-1 flex-col gap-0.5">
                  <span class="truncate font-mono text-xs text-fg">{p.name}</span>
                  <span class="truncate text-xs text-fg-subtle">{portDescription(p)}</span>
                </div>
                {#if s}
                  <Badge tone={statusTone(s)} dot>{s.chipInfo?.chipName ?? statusLabel(s)}</Badge>
                {:else if hint}
                  <Badge tone={hint.tone}>{hint.label}</Badge>
                {/if}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
      <details class="border-t border-border px-4 py-3 text-xs text-fg-muted">
        <summary class="cursor-default text-fg-subtle hover:text-fg-muted">Device not listed?</summary>
        <ul class="mt-2 flex list-disc flex-col gap-1 pl-4">
          <li>Use a USB cable that carries data, not only power.</li>
          <li>Install the driver for the board's USB-to-UART bridge (Silicon Labs CP210x, WCH CH34x or FTDI).</li>
          <li>Chips with native USB (ESP32-S3, -C3, -C6, …) appear as an Espressif USB device.</li>
          <li>Close other serial monitors or IDEs that may hold the port.</li>
          <li>OxidizeESP does not support the ESP8266.</li>
        </ul>
      </details>
    </Panel>

    <div class="flex min-w-0 flex-col gap-5">
      {#if !port}
        <Panel>
          <EmptyState
            icon={Cpu}
            title="No device selected"
            description="Choose a serial port to see its details, then connect to identify the chip."
          />
        </Panel>
      {:else}
        <Panel title={session ? deviceTitle(session) : port.name} description={session ? session.port.name : portDescription(port)}>
          {#snippet actions()}
            {#if session}<Badge tone={statusTone(session)} dot>{statusLabel(session)}</Badge>{/if}
          {/snippet}
          <div class="flex flex-col gap-4">
            {#if !session}
              <Callout tone="neutral" title="Not connected">
                Connecting resets the chip into its bootloader to identify it, then restarts the application. Serial port
                details below are reported by the operating system and do not confirm that an ESP chip is attached.
              </Callout>
            {:else if !info && session.status !== 'disconnected'}
              <Callout tone="warning" title="Chip not identified">
                Serial port details are known, but the chip has not been verified yet.
              </Callout>
            {/if}
            {#if session?.previousPortName}
              <Callout tone="info">
                This device was previously on <span class="font-mono">{session.previousPortName}</span> and was matched by its
                USB serial number.
              </Callout>
            {/if}

            <div class="flex flex-wrap gap-2">
              {#if !session || session.status === 'disconnected'}
                <ActionButton
                  variant="primary"
                  loading={devices.connectJob.running}
                  disabledReason={session?.status === 'disconnected' ? 'The device is not connected to this port.' : null}
                  onclick={() => devices.connect(port.name)}
                >
                  <Plug />
                  Connect and identify
                </ActionButton>
              {:else}
                <ActionButton
                  variant={info ? 'secondary' : 'primary'}
                  loading={devices.connectJob.running}
                  disabledReason={operationBlocker}
                  onclick={() => devices.identify(session.id)}
                >
                  <Cpu />
                  {info ? 'Identify again' : 'Identify'}
                </ActionButton>
                <ActionButton
                  disabledReason={operationBlocker}
                  title="Restart the chip using the DTR/RTS lines"
                  onclick={reset}
                >
                  <RotateCcw />
                  Reset
                </ActionButton>
                <ActionButton
                  disabledReason={operationBlocker}
                  title="Enter the ROM bootloader so the application stops running"
                  onclick={holdInReset}
                >
                  <Power />
                  Hold in reset
                </ActionButton>
                <ActionButton
                  variant="ghost"
                  disabledReason={session.activity.type === 'operation' ? 'Wait for the current operation to finish.' : null}
                  onclick={() => devices.disconnect(session.id)}
                >
                  <Unplug />
                  Disconnect
                </ActionButton>
              {/if}
            </div>

            {#if session?.lastError && devices.connectJob.state.phase !== 'running'}
              <ErrorCallout error={session.lastError} title="Last operation failed" />
            {/if}
            <JobPanel runner={devices.connectJob} />
            <JobPanel runner={job} />
          </div>
        </Panel>

        {#if info}
          <Panel title="Chip" description="Verified by communicating with the ROM bootloader.">
            <KeyValue rows={chipRows} />
          </Panel>
          <Panel title="Security" description="Reported by the bootloader where supported.">
            {#if securityRows.length}
              <KeyValue rows={securityRows} />
              {#if info.security?.flashEncryption || info.security?.secureBoot}
                <Callout tone="warning" class="mt-3">
                  Security features are enabled. Flash backups contain device-specific data and may not be restorable to
                  other devices; some operations can render the device unusable.
                </Callout>
              {/if}
            {:else}
              <p class="text-[13px] text-fg-subtle">This chip's bootloader does not report security information.</p>
            {/if}
          </Panel>
        {/if}

        <Panel title="Serial port" description="Reported by the operating system.">
          <KeyValue rows={portRows} />
        </Panel>
      {/if}
    </div>
  </div>
</ViewScroll>
