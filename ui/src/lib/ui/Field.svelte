<script lang="ts">
  import type { Snippet } from "svelte";
  let { label, children }: { label: string; children?: Snippet } = $props();
</script>

<label class="flex flex-col gap-1.5 text-text-muted text-sm font-medium">
  <span>{label}</span>
  {@render children?.()}
</label>
