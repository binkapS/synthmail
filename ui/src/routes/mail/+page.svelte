<script lang="ts">
  import { goto } from "$app/navigation";
  import * as Api from "$lib/api";
  import AuthGuard from "$lib/components/AuthGuard.svelte";
  import { Card, Spinner } from "$lib/ui";

  let loading = $state(true);
  $effect(() => {
    if (!Api.getToken()) { goto("/login"); return; }
    loading = false;
  });
</script>

<AuthGuard>
  {#if loading}
    <div class="p-6"><Spinner /></div>
  {:else}
    <div class="flex h-[calc(100vh-8rem)] w-full overflow-hidden bg-appBg text-text-primary rounded-2xl border border-divider">
      <aside class="w-[18%] bg-surface border-r border-divider flex flex-col p-4 gap-4">
        <div class="font-bold">Navigation</div>
        <p class="text-text-muted text-sm">Webmail scaffold (Phase 4).</p>
      </aside>
      <section class="w-[32%] bg-appBg border-r border-divider flex flex-col overflow-y-auto p-4">
        <p class="text-text-muted text-sm">Mailbox list</p>
      </section>
      <main class="flex-1 bg-surface flex flex-col p-6 overflow-y-auto">
        <p class="text-text-muted text-sm">Reading pane</p>
      </main>
    </div>
  {/if}
</AuthGuard>
