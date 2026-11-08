<script lang="ts">
  import "../app.css";
  import TopBar from "$lib/components/TopBar.svelte";
  import BottomNav from "$lib/components/BottomNav.svelte";

  let { children } = $props();
</script>

<div class="bg-appBg text-text-primary min-h-screen">
  <div class="max-w-7xl mx-auto px-4 py-4 pb-24 md:pb-4">
    <TopBar />
    <main class="mt-4">
      {@render children()}
    </main>
  </div>
  <BottomNav />
</div>
