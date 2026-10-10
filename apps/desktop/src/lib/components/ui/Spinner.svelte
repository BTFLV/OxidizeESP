<script lang="ts">
  import { LoaderCircle } from '$lib/icons';
  import { cn } from '$lib/utils/cn';

  let { class: className = '' }: { class?: string } = $props();
</script>

<LoaderCircle class={cn('size-4 animate-spin', className)} aria-hidden="true" />
