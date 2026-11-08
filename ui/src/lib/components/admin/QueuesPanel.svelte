<script lang="ts">
  import * as Api from "$lib/api";
  import { Card, Spinner } from "$lib/ui";

  let data = $state<any>(null);
  let loading = $state(true);
  let err = $state("");

  async function load() {
    loading = true;
    err = "";
    try {
      data = await Api.queues();
    } catch (e: any) {
      err = e?.message ?? String(e);
    } finally {
      loading = false;
    }
  }
  $effect(() => { load(); });
</script>

<Card>
  <div class="flex items-center justify-between gap-3 flex-wrap">
    <h2 class="text-xl font-semibold">Queues</h2>
    <button onclick={load} class="bg-surface border border-divider hover:bg-elevated text-text-muted hover:text-text-primary px-5 py-2.5 rounded-xl transition-colors">Refresh</button>
  </div>
  {#if err}<p class="text-danger text-sm">{err}</p>{/if}
  {#if loading}
    <Spinner />
  {:else if data}
    <div class="grid grid-cols-1 md:grid-cols-4 gap-4 mt-3">
      <div class="bg-appBg border border-divider rounded-xl p-4"><span class="text-text-muted text-xs">Deferred</span><div class="text-2xl font-bold">{data.queue?.deferred ?? 0}</div></div>
      <div class="bg-appBg border border-divider rounded-xl p-4"><span class="text-text-muted text-xs">Active</span><div class="text-2xl font-bold">{data.queue?.active ?? 0}</div></div>
      <div class="bg-appBg border border-divider rounded-xl p-4"><span class="text-text-muted text-xs">Hold</span><div class="text-2xl font-bold">{data.queue?.hold ?? 0}</div></div>
      <div class="bg-appBg border border-divider rounded-xl p-4"><span class="text-text-muted text-xs">Postfix size</span><div class="text-2xl font-bold">{data.postfix_queue_size ?? 0}</div></div>
    </div>
    <p class="text-text-muted text-sm mt-3">Activity 1h: {data.activity_1h ?? 0}</p>
  {/if}
</Card>
