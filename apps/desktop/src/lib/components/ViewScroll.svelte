<!-- Scrollable page body with consistent padding and width. -->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { cn } from '$lib/utils/cn';

  let { children, class: className }: { children: Snippet; class?: string } = $props();
</script>

<div class="h-full overflow-y-auto">
  <div class={cn('mx-auto flex max-w-[1100px] flex-col gap-5 px-6 py-5', className)}>
    {@render children()}
  </div>
</div>
