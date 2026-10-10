<script lang="ts">
  import { Check, Copy } from '$lib/icons';
  import Button from './Button.svelte';
  import Tooltip from './Tooltip.svelte';

  let { text, label = 'Copy' }: { text: string; label?: string } = $props();
  let copied = $state(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      copied = false;
    }
  }
</script>

<Tooltip text={copied ? 'Copied' : label}>
  {#snippet children(props)}
    <Button {...props} variant="ghost" size="icon-sm" onclick={copy} aria-label={label}>
      {#if copied}<Check class="text-success" />{:else}<Copy />{/if}
    </Button>
  {/snippet}
</Tooltip>
