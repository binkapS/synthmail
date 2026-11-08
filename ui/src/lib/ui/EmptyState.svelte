<script lang="ts">
  let { text }: { text: string } = $props();
</script>

<div class="bg-surface border border-divider rounded-2xl p-6 text-center">
  <p class="text-text-muted">{text}</p>
</div>
