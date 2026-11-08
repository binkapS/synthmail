<script lang="ts">
  import * as Api from "$lib/api";
  import { Card, Spinner, EmptyState } from "$lib/ui";

  let rows = $state<any[]>([]);
  let total = $state(0);
  let used = $state(0);
  let loading = $state(true);
  let err = $state("");

  async function load() {
    loading = true;
    err = "";
    try {
      const q: any = await Api.quota();
      total = Number(q.quota_mb ?? 0);
      used = Number(q.used_mb ?? 0);
      rows = q.users ?? [];
    } catch (e: any) {
      err = e?.message ?? String(e);
    } finally {
      loading = false;
    }
  }
  $effect(() => { load(); });
</script>

<Card>
  <h2 class="text-xl font-semibold">Quota</h2>
  {#if err}<p class="text-danger text-sm">{err}</p>{/if}
  {#if loading}
    <Spinner />
  {:else}
    <div class="grid grid-cols-1 md:grid-cols-3 gap-6 p-6 bg-appBg rounded-xl mt-3">
      <div class="bg-surface border border-divider rounded-xl p-5 flex flex-col gap-2"><span class="text-text-muted text-sm">Total quota</span><span class="text-2xl font-bold">{total} MB</span></div>
      <div class="bg-surface border border-divider rounded-xl p-5 flex flex-col gap-2"><span class="text-text-muted text-sm">Used</span><span class="text-2xl font-bold">{used} MB</span></div>
      <div class="bg-surface border border-divider rounded-xl p-5 flex flex-col gap-2"><span class="text-text-muted text-sm">Usage</span><span class="text-2xl font-bold">{total ? ((used/total)*100).toFixed(1) : 0}%</span></div>
    </div>
    {#if rows.length === 0}
      <EmptyState text="No quota data." />
    {:else}
      <div class="overflow-x-auto mt-3">
        <table class="w-full text-sm">
          <thead class="text-text-muted border-b border-divider"><tr><th class="text-left py-2">Email</th><th class="text-left py-2">Quota</th><th class="text-left py-2">Used</th><th class="text-left py-2">%</th></tr></thead>
          <tbody>
            {#each rows as row}
              <tr class="border-b border-divider">
                <td class="py-2">{row.email}</td>
                <td class="py-2">{row.quota_mb} MB</td>
                <td class="py-2">{row.used_mb} MB</td>
                <td class="py-2">
                  <div class="flex items-center gap-2">
                    <div class="flex-1 h-2 bg-elevated rounded-full overflow-hidden">
                      <div class="h-2 bg-brand rounded-full" style="width: {Math.min(100, row.pct ?? (row.quota_mb ? row.used_mb/row.quota_mb*100 : 0))}%"></div>
                    </div>
                    {(row.pct ?? (row.quota_mb ? (row.used_mb / row.quota_mb * 100) : 0)).toFixed(0)}%
                  </div>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  {/if}
</Card>
