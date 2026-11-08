<script lang="ts">
  import * as Api from "$lib/api";
  import { Card, Spinner, EmptyState } from "$lib/ui";

  let rows = $state<any[]>([]);
  let loading = $state(true);
  let err = $state("");

  async function load() {
    loading = true;
    err = "";
    try { rows = await Api.bans(); } catch (e: any) { err = e?.message ?? String(e); }
    finally { loading = false; }
  }
  $effect(() => { load(); });
</script>

<Card>
  <div class="flex items-center justify-between gap-3 flex-wrap">
    <h2 class="text-xl font-semibold">Bans</h2>
    <button onclick={load} class="bg-surface border border-divider hover:bg-elevated text-text-muted hover:text-text-primary px-5 py-2.5 rounded-xl transition-colors">Refresh</button>
  </div>
  {#if err}<p class="text-danger text-sm">{err}</p>{/if}
  {#if loading}
    <Spinner />
  {:else if rows.length === 0}
    <EmptyState text="No bans." />
  {:else}
    <div class="overflow-x-auto mt-3">
      <table class="w-full text-sm">
        <thead class="text-text-muted border-b border-divider"><tr><th class="text-left py-2">IP / Key</th><th class="text-left py-2">Reason</th><th class="text-left py-2">Created</th></tr></thead>
        <tbody>
          {#each rows as row}
            <tr class="border-b border-divider">
              <td class="py-2">{row.ip ?? row.key ?? row.id ?? ""}</td>
              <td class="py-2">{row.reason ?? row.event ?? ""}</td>
              <td class="py-2">{row.created_at ?? row.time ?? ""}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</Card>
