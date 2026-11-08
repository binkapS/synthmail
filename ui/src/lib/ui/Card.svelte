<script lang="ts">
  import type { Snippet } from "svelte";
  let { class: className = "", children }: { class?: string; children?: Snippet } = $props();
</script>

<div class="bg-surface border border-divider rounded-2xl p-6 hover:bg-surface/80 hover:border-text-dim/30 hover:-translate-y-0.5 shadow-xl transition-all duration-200 {className}">
  {@render children?.()}
</div>
