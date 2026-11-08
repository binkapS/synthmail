<script lang="ts">
  import "../app.css";
  import TopBar from "$lib/components/TopBar.svelte";
  import BottomNav from "$lib/components/BottomNav.svelte";

  let { children } = $props();
</script>

<div class="min-h-screen text-text-primary">
  <div class="max-w-[1500px] mx-auto px-3 sm:px-5 py-3 pb-28 md:pb-4">
    <TopBar />
    <main class="mt-4 animate-in fade-in">
      {@render children()}
    </main>
  </div>
  <BottomNav />
</div>
