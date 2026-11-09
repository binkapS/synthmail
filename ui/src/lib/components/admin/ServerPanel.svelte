<script lang="ts">
  import * as Api from "$lib/api";
  import { Card, Spinner } from "$lib/ui";

  let server = $state<any>(null);
  let loading = $state(true);
  let err = $state("");

  async function load() {
    loading = true;
    err = "";
    try {
      server = await Api.serverStats();
    } catch (e: any) {
      err = e?.message ?? String(e);
    } finally {
      loading = false;
    }
  }

  function fmtBytes(n: number) {
    if (!n) return "—";
    const gb = n / 1024 / 1024 / 1024;
    if (gb >= 1) return `${gb.toFixed(1)} GB`;
    const mb = n / 1024 / 1024;
    return `${mb.toFixed(0)} MB`;
  }

  $effect(() => { load(); });
</script>

<Card>
  <div class="flex items-center justify-between gap-3 flex-wrap">
    <h2 class="text-xl font-semibold">Server details & stats</h2>
    <button onclick={load} class="bg-surface border border-divider hover:bg-elevated text-text-muted hover:text-text-primary px-5 py-2.5 rounded-xl transition-colors">Refresh</button>
  </div>
  {#if err}<p class="text-danger text-sm">{err}</p>{/if}
  {#if loading}
    <Spinner />
  {:else if server}
    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 mt-3">
      <div class="bg-appBg border border-divider rounded-xl p-4">
        <div class="text-xs text-text-muted uppercase tracking-wider">Host</div>
        <div class="mt-1 text-sm text-text-primary">{server.hostname ?? "—"}</div>
        <div class="text-xs text-text-dim">{server.platform ?? "linux"} · {server.cpu_cores ?? 0} cores</div>
      </div>
      <div class="bg-appBg border border-divider rounded-xl p-4">
        <div class="text-xs text-text-muted uppercase tracking-wider">Load average</div>
        <div class="mt-1 text-sm text-text-primary">{server.loadavg_1?.toFixed?.(2) ?? 0} / {server.loadavg_5?.toFixed?.(2) ?? 0} / {server.loadavg_15?.toFixed?.(2) ?? 0}</div>
        <div class="text-xs text-text-dim">1m / 5m / 15m</div>
      </div>
      <div class="bg-appBg border border-divider rounded-xl p-4">
        <div class="text-xs text-text-muted uppercase tracking-wider">Memory</div>
        <div class="mt-1 text-sm text-text-primary">{fmtBytes(server.memory_used_bytes ?? 0)} used</div>
        <div class="text-xs text-text-dim">of {fmtBytes(server.memory_total_bytes ?? 0)} · avail {fmtBytes(server.memory_available_bytes ?? 0)}</div>
        <div class="mt-2 h-2 bg-elevated rounded-full overflow-hidden">
          <div class="h-2 bg-brand rounded-full" style="width: {server.memory_total_bytes ? (server.memory_used_bytes / server.memory_total_bytes * 100) : 0}%"></div>
        </div>
      </div>
      {#each server.disks ?? [] as d}
        <div class="bg-appBg border border-divider rounded-xl p-4">
          <div class="text-xs text-text-muted uppercase tracking-wider">Disk {d.path}</div>
          <div class="mt-1 text-sm text-text-primary">{fmtBytes(d.used_bytes ?? 0)} used</div>
          <div class="text-xs text-text-dim">of {fmtBytes(d.total_bytes ?? 0)} · avail {fmtBytes(d.available_bytes ?? 0)}</div>
          <div class="mt-2 h-2 bg-elevated rounded-full overflow-hidden">
            <div class="h-2 bg-info rounded-full" style="width: {d.total_bytes ? (d.used_bytes / d.total_bytes * 100) : 0}%"></div>
          </div>
        </div>
      {/each}
    </div>

    <div class="border-t border-divider mt-6 pt-4">
      <h3 class="font-semibold mb-2">Docker containers</h3>
      <p class="text-xs text-text-dim mb-3">{server.containers_note ?? ""}</p>
      {#if (server.containers ?? []).length === 0}
        <p class="text-text-muted text-sm">
          {server.containers_note?.toLowerCase().includes("intentionally disabled")
            ? "Container view is intentionally off. Enable it with ENABLE_CONTAINER_STATS=1 and a /var/run/docker.sock mount when you need per-container status. Host-level metrics above are unaffected."
            : "No container data available for this server."}
        </p>
      {:else}
        <div class="flex flex-col gap-2">
          {#each server.containers as c}
            <div class="flex items-center justify-between gap-3 rounded-xl border border-divider bg-appBg/40 p-3">
              <span class="text-sm font-medium">{c.name || "—"}</span>
              <span class="text-xs text-text-muted">{c.status || c.state}</span>
              <span class="text-xs px-2 py-0.5 rounded-full border {c.state === 'running' ? 'bg-success/10 text-success border-success/20' : 'bg-danger/10 text-danger border-danger/20'}">{c.state || "—"}</span>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</Card>
