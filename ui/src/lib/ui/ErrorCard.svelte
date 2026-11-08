<script lang="ts">
  let { error }: { error: string } = $props();
</script>

<div class="bg-surface border border-danger/20 rounded-2xl p-4" role="alert">
  <p class="text-danger">{error}</p>
</div>
