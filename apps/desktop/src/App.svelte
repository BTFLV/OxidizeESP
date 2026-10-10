<script lang="ts">
  import { Tooltip } from 'bits-ui';
  import { onMount } from 'svelte';
  import Sidebar from '$lib/components/layout/Sidebar.svelte';
  import StatusBar from '$lib/components/layout/StatusBar.svelte';
  import TopBar from '$lib/components/layout/TopBar.svelte';
  import Toaster from '$lib/components/Toaster.svelte';
  import { app } from '$lib/stores/app.svelte';
  import { devices } from '$lib/stores/devices.svelte';
  import { dropZones } from '$lib/stores/dropzones.svelte';
  import { monitor } from '$lib/stores/monitor.svelte';
  import { nav } from '$lib/stores/nav.svelte';
  import { resolveTheme, settings } from '$lib/stores/settings.svelte';
  import { toasts } from '$lib/stores/toasts.svelte';
  import { toAppError } from '$lib/utils/errors';
  import BackupView from '$lib/views/BackupView.svelte';
  import DashboardView from '$lib/views/DashboardView.svelte';
  import EraseView from '$lib/views/EraseView.svelte';
  import FlashView from '$lib/views/FlashView.svelte';
  import ImageToolsView from '$lib/views/ImageToolsView.svelte';
  import MonitorView from '$lib/views/MonitorView.svelte';
  import PartitionsView from '$lib/views/PartitionsView.svelte';
  import SettingsView from '$lib/views/SettingsView.svelte';
  import { VIEWS } from '$lib/navigation';

  let prefersDark = $state(true);
  let startupError = $state<string | null>(null);

  $effect(() => {
    document.documentElement.dataset.theme = resolveTheme(settings.value.appearance, prefersDark);
  });

  $effect(() => {
    monitor.setCapacity(settings.value.monitor.maxLines);
  });

  function onKeydown(event: KeyboardEvent) {
    const mod = app.isMac ? event.metaKey : event.ctrlKey;
    if (!mod || event.altKey || event.shiftKey) return;
    const index = Number(event.key) - 1;
    const view = VIEWS[index];
    if (Number.isInteger(index) && view) {
      event.preventDefault();
      nav.go(view.id);
    }
  }

  onMount(() => {
    const media = window.matchMedia('(prefers-color-scheme: dark)');
    prefersDark = media.matches;
    const onMedia = (e: MediaQueryListEvent) => (prefersDark = e.matches);
    media.addEventListener('change', onMedia);

    (async () => {
      try {
        await settings.load();
        await app.load();
        await dropZones.init();
        await devices.init();
        if (app.info?.settingsWarning) toasts.show('warning', 'Settings were reset', app.info.settingsWarning, 10000);
      } catch (error) {
        startupError = toAppError(error).message;
      }
    })();

    return () => {
      media.removeEventListener('change', onMedia);
      devices.dispose();
    };
  });
</script>

<svelte:window onkeydown={onKeydown} />

<Tooltip.Provider>
  <div class="flex h-full flex-col">
    <div class="flex min-h-0 flex-1">
      <Sidebar />
      <div class="flex min-w-0 flex-1 flex-col">
        <TopBar />
        {#if startupError}
          <div class="border-b border-danger/30 bg-danger-soft px-4 py-2 text-xs text-danger" role="alert">
            OxidizeESP could not finish starting: {startupError}
          </div>
        {/if}
        <main class="min-h-0 flex-1 overflow-hidden bg-bg">
          <div class="h-full" hidden={nav.view !== 'dashboard'}><DashboardView /></div>
          <div class="h-full" hidden={nav.view !== 'flash'}><FlashView /></div>
          <div class="h-full" hidden={nav.view !== 'backup'}><BackupView /></div>
          <div class="h-full" hidden={nav.view !== 'erase'}><EraseView /></div>
          <div class="h-full" hidden={nav.view !== 'monitor'}><MonitorView active={nav.view === 'monitor'} /></div>
          <div class="h-full" hidden={nav.view !== 'partitions'}><PartitionsView /></div>
          <div class="h-full" hidden={nav.view !== 'image'}><ImageToolsView /></div>
          <div class="h-full" hidden={nav.view !== 'settings'}><SettingsView /></div>
        </main>
      </div>
    </div>
    <StatusBar />
  </div>
  <Toaster />
</Tooltip.Provider>
