<script lang="ts">
  import type { Snippet } from "svelte";
  let { label, class: className = "", children }: { label: string; class?: string; children?: Snippet } = $props();
</script>

<label class="flex flex-col gap-1.5 text-text-muted text-sm font-medium {className}">
  <span>{label}</span>
  {@render children?.()}
</label>
