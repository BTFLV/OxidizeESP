<script lang="ts">
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { api } from '$lib/api';
  import type { Appearance } from '$lib/bindings/Appearance';
  import type { LineEnding } from '$lib/bindings/LineEnding';
  import type { LogLevel } from '$lib/bindings/LogLevel';
  import type { PortSelection } from '$lib/bindings/PortSelection';
  import type { Settings } from '$lib/bindings/Settings';
  import PageHeader from '$lib/components/PageHeader.svelte';
  import Button from '$lib/components/ui/Button.svelte';
  import Callout from '$lib/components/ui/Callout.svelte';
  import ErrorCallout from '$lib/components/ui/ErrorCallout.svelte';
  import Field from '$lib/components/ui/Field.svelte';
  import KeyValue from '$lib/components/ui/KeyValue.svelte';
  import Panel from '$lib/components/ui/Panel.svelte';
  import Segmented from '$lib/components/ui/Segmented.svelte';
  import Select from '$lib/components/ui/Select.svelte';
  import Switch from '$lib/components/ui/Switch.svelte';
  import ViewScroll from '$lib/components/ViewScroll.svelte';
  import { FolderOpen, Monitor, Moon, Sun, X } from '$lib/icons';
  import { BAUD_RATES, MONITOR_BAUD_RATES } from '$lib/options';
  import { app } from '$lib/stores/app.svelte';
  import { settings } from '$lib/stores/settings.svelte';
  import { toasts } from '$lib/stores/toasts.svelte';
  import { toAppError } from '$lib/utils/errors';

  const s = $derived(settings.value);

  async function change(mutate: (draft: Settings) => void) {
    const error = await settings.update(mutate);
    if (error) toasts.show('danger', 'The setting could not be saved', error.message);
  }

  async function chooseFolder(key: 'firmwareDir' | 'backupDir') {
    const result = await openDialog({ directory: true, multiple: false, defaultPath: s[key] ?? undefined });
    if (typeof result === 'string') await change((d) => (d[key] = result));
  }

  async function openLogs() {
    try {
      await api.openLogFolder();
    } catch (error) {
      toasts.show('danger', toAppError(error).message);
    }
  }

  const bufferOptions = [1_000, 5_000, 10_000, 20_000, 50_000, 100_000, 200_000];
</script>

<ViewScroll class="max-w-3xl">
  <PageHeader title="Settings" description="Preferences are stored on this computer and apply immediately." />
  {#if settings.error}<ErrorCallout error={settings.error} />{/if}

  <Panel title="Appearance">
    <Field label="Theme" id="theme">
      <Segmented
        label="Theme"
        value={s.appearance}
        options={[
          { value: 'dark', label: 'Dark', icon: Moon },
          { value: 'light', label: 'Light', icon: Sun },
          { value: 'system', label: 'System', icon: Monitor },
        ]}
        onValueChange={(v: Appearance) => change((d) => (d.appearance = v))}
        class="self-start"
      />
    </Field>
  </Panel>

  <Panel title="Connection" description="Defaults for flashing, backups and other bootloader operations.">
    <div class="flex flex-col gap-4">
      <Field label="Default baud rate" id="set-baud" hint="Lower the rate if transfers fail with long or low-quality cables.">
        <Select
          id="set-baud"
          class="w-56"
          items={BAUD_RATES.map((b) => ({ value: String(b), label: b.toLocaleString('en-US') }))}
          value={String(s.flashBaud)}
          onValueChange={(v) => change((d) => (d.flashBaud = Number(v)))}
        />
      </Field>
      <Switch
        label="Use flasher stub"
        description="Faster transfers and required for erasing. Disable only for troubleshooting."
        checked={s.useStub}
        onCheckedChange={(v) => change((d) => (d.useStub = v))}
      />
      <Switch
        label="Verify after flashing"
        description="Compare a device-side checksum after every write."
        checked={s.verifyAfterFlash}
        onCheckedChange={(v) => change((d) => (d.verifyAfterFlash = v))}
      />
      <Field label="Port preselection" id="set-port">
        <Select
          id="set-port"
          class="w-80"
          items={[
            { value: 'rememberLast', label: 'Remember the last used port' },
            { value: 'autoSelectSingle', label: 'Select automatically if only one board is attached' },
            { value: 'manual', label: 'Always select manually' },
          ]}
          value={s.portSelection}
          onValueChange={(v) => change((d) => (d.portSelection = v as PortSelection))}
        />
      </Field>
    </div>
  </Panel>

  <Panel title="Folders" description="Starting folders for file dialogs.">
    <div class="flex flex-col gap-4">
      {#each [{ key: 'firmwareDir', label: 'Firmware folder' }, { key: 'backupDir', label: 'Backup folder' }] as const as folder (folder.key)}
        <Field label={folder.label}>
          <div class="flex items-center gap-2">
            <span class="min-w-0 flex-1 truncate rounded-md border border-border bg-surface px-2.5 py-1.5 font-mono text-xs text-fg-muted" title={s[folder.key] ?? undefined}>
              {s[folder.key] ?? 'Not set'}
            </span>
            {#if s[folder.key]}
              <Button variant="ghost" size="icon-sm" aria-label="Clear {folder.label.toLowerCase()}" onclick={() => change((d) => (d[folder.key] = null))}><X /></Button>
            {/if}
            <Button size="sm" onclick={() => chooseFolder(folder.key)}><FolderOpen /> Choose…</Button>
          </div>
        </Field>
      {/each}
    </div>
  </Panel>

  <Panel title="Serial monitor">
    <div class="flex flex-col gap-4">
      <div class="grid grid-cols-3 gap-3">
        <Field label="Default baud rate" id="set-mon-baud">
          <Select
            id="set-mon-baud"
            items={MONITOR_BAUD_RATES.map((b) => ({ value: String(b), label: b.toLocaleString('en-US') }))}
            value={String(s.monitor.baud)}
            onValueChange={(v) => change((d) => (d.monitor.baud = Number(v)))}
          />
        </Field>
        <Field label="Buffer size" id="set-mon-buffer" hint="Older lines are discarded.">
          <Select
            id="set-mon-buffer"
            items={bufferOptions.map((n) => ({ value: String(n), label: `${n.toLocaleString('en-US')} lines` }))}
            value={String(s.monitor.maxLines)}
            onValueChange={(v) => change((d) => (d.monitor.maxLines = Number(v)))}
          />
        </Field>
        <Field label="Line ending" id="set-mon-eol">
          <Select
            id="set-mon-eol"
            items={[
              { value: 'lf', label: 'LF' },
              { value: 'crLf', label: 'CR LF' },
              { value: 'cr', label: 'CR' },
              { value: 'none', label: 'None' },
            ]}
            value={s.monitor.lineEnding}
            onValueChange={(v) => change((d) => (d.monitor.lineEnding = v as LineEnding))}
          />
        </Field>
      </div>
      <Switch label="Show timestamps" checked={s.monitor.showTimestamps} onCheckedChange={(v) => change((d) => (d.monitor.showTimestamps = v))} />
      <Switch label="Autoscroll" checked={s.monitor.autoscroll} onCheckedChange={(v) => change((d) => (d.monitor.autoscroll = v))} />
      <Switch
        label="Reset the device when the monitor starts"
        description="Captures the boot log from the beginning."
        checked={s.monitor.resetOnStart}
        onCheckedChange={(v) => change((d) => (d.monitor.resetOnStart = v))}
      />
      <Switch
        label="Reconnect automatically"
        description="Resume monitoring when the same device is plugged in again. The port is only reopened, not reset."
        checked={s.monitor.autoReconnect}
        onCheckedChange={(v) => change((d) => (d.monitor.autoReconnect = v))}
      />
      <Switch
        label="Decode addresses"
        description="Resolve code addresses to functions and source lines using the selected ELF file."
        checked={s.monitor.decodeAddresses}
        onCheckedChange={(v) => change((d) => (d.monitor.decodeAddresses = v))}
      />
    </div>
  </Panel>

  <Panel title="Interface">
    <Switch
      label="Show advanced options by default"
      description="Expand flash parameters, bootloader and connection options in operation forms."
      checked={s.showAdvanced}
      onCheckedChange={(v) => change((d) => (d.showAdvanced = v))}
    />
  </Panel>

  <Panel title="Logging" description="Application logs help diagnose connection problems. They contain no telemetry and stay on this computer.">
    <div class="flex items-end gap-3">
      <Field label="Log level" id="set-log" class="w-48">
        <Select
          id="set-log"
          items={[
            { value: 'error', label: 'Errors' },
            { value: 'warn', label: 'Warnings' },
            { value: 'info', label: 'Information' },
            { value: 'debug', label: 'Debug' },
          ]}
          value={s.logLevel}
          onValueChange={(v) => change((d) => (d.logLevel = v as LogLevel))}
        />
      </Field>
      <Button onclick={openLogs}><FolderOpen /> Open log folder</Button>
    </div>
  </Panel>

  <Panel title="About">
    <div class="flex flex-col gap-4">
      <div class="flex items-center gap-3">
        <img src="/icon.svg" alt="" class="size-10" />
        <div>
          <p class="text-[15px] font-semibold text-fg">OxidizeESP</p>
          <p class="text-xs text-fg-subtle">Flash, monitor and manage Espressif microcontrollers.</p>
        </div>
      </div>
      {#if app.info}
        <KeyValue
          rows={[
            { label: 'Version', value: app.info.version },
            { label: 'espflash', value: app.info.espflashVersion },
            { label: 'Platform', value: `${app.info.os} (${app.info.arch})` },
            { label: 'Hardware access', value: app.info.simulatedHardware ? 'Simulated (development build)' : 'Real devices' },
          ]}
        />
      {/if}
      {#if app.info?.simulatedHardware}
        <Callout tone="warning">This build uses simulated devices. It cannot communicate with real hardware.</Callout>
      {/if}
      <p class="text-xs text-fg-subtle">
        OxidizeESP is an independent open-source project built on the espflash library from the esp-rs community. It is not
        affiliated with or endorsed by Espressif Systems. ESP32 is a trademark of Espressif Systems.
      </p>
    </div>
  </Panel>
</ViewScroll>
