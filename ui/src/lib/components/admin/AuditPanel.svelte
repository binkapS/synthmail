<script lang="ts">
  import * as Api from "$lib/api";
  import { Card, Spinner, EmptyState } from "$lib/ui";

  let rows = $state<any[]>([]);
  let loading = $state(true);
  let err = $state("");
  let limit = $state("50");

  async function load() {
    loading = true;
    err = "";
    try {
      rows = await Api.audit(parseInt(limit || "50", 10) || 50);
    } catch (e: any) { err = e?.message ?? String(e); }
    finally { loading = false; }
  }
  $effect(() => { load(); });
</script>

<Card>
  <div class="flex items-center justify-between gap-3 flex-wrap">
    <h2 class="text-xl font-semibold">Audit</h2>
    <div class="flex gap-2 items-center">
      <input bind:value={limit} class="bg-appBg border border-divider text-text-primary rounded-xl px-3 py-2 w-20" />
      <button onclick={load} class="bg-surface border border-divider hover:bg-elevated text-text-muted hover:text-text-primary px-5 py-2.5 rounded-xl transition-colors">Apply</button>
    </div>
  </div>
  {#if err}<p class="text-danger text-sm">{err}</p>{/if}
  {#if loading}
    <Spinner />
  {:else if rows.length === 0}
    <EmptyState text="No audit entries." />
  {:else}
    <div class="overflow-x-auto mt-3">
      <table class="w-full text-sm">
        <thead class="text-text-muted border-b border-divider"><tr><th class="text-left py-2">Actor</th><th class="text-left py-2">Action</th><th class="text-left py-2">Target</th><th class="text-left py-2">Time</th></tr></thead>
        <tbody>
          {#each rows as row}
            <tr class="border-b border-divider">
              <td class="py-2">{row.actor ?? row.user ?? row.email ?? ""}</td>
              <td class="py-2">{row.action ?? row.event ?? ""}</td>
              <td class="py-2">{row.target ?? row.object ?? row.id ?? ""}</td>
              <td class="py-2">{row.time ?? row.created_at ?? row.when ?? ""}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</Card>
