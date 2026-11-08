<script lang="ts">
  import "../app.css";

  let { children } = $props();
</script>

<div class="bg-appBg text-text-primary min-h-screen">
  {@render children()}
</div>
