<script lang="ts">
  import { Tooltip } from 'bits-ui';
  import ActionButton from '$lib/components/ui/ActionButton.svelte';

  let { onclick, disabledReason }: { onclick: () => void; disabledReason: string | null } = $props();
</script>

<Tooltip.Provider>
  <ActionButton {onclick} {disabledReason}>Flash</ActionButton>
</Tooltip.Provider>
