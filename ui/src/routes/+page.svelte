<script lang="ts">
  import { goto } from "$app/navigation";
  import * as Api from "$lib/api";
  import AuthGuard from "$lib/components/AuthGuard.svelte";
  import { Spinner, EmptyState } from "$lib/ui";
  // Note: keep these even if unused? remove unused imports

  let domains = $state(0);
  let users = $state(0);
  let quotaUsed = $state("");
  let quotaTotal = $state("");
  let deferred = $state(0);
  let loading = $state(true);
  let failed = $state(false);
  let auditRows = $state<any[]>([]);

  $effect(() => {
    if (!Api.getToken()) { goto("/login"); return; }
    let cancelled = false;
    (async () => {
      let anyFail = false;
      try { domains = (await Api.listDomains()).length; } catch { anyFail = true; }
      try { users = (await Api.listUsers()).length; } catch { anyFail = true; }
      try { const r: any = await Api.quota(); quotaUsed = String(r.used_mb ?? ""); quotaTotal = String(r.quota_mb ?? ""); } catch { anyFail = true; }
      try { const r: any = await Api.queues(); deferred = Number(r?.queue?.deferred ?? 0); } catch { anyFail = true; }
      try { auditRows = (await Api.audit(6)).slice(0, 6); } catch { /* ok */ }
      if (!cancelled) { failed = anyFail; loading = false; }
    })();
    return () => { cancelled = true; };
  });
</script>

<AuthGuard>
  <div class="space-y-6">
  <section class="relative overflow-hidden rounded-3xl border border-divider bg-gradient-to-br from-surface via-surface to-elevated p-6 sm:p-8 shadow-2xl shadow-black/30">
    <div class="absolute -top-20 -right-20 w-72 h-72 rounded-full bg-brand/20 blur-3xl pointer-events-none"></div>
    <div class="absolute bottom-0 left-1/3 w-60 h-60 rounded-full bg-info/15 blur-3xl pointer-events-none"></div>
    <div class="relative">
      <span class="inline-flex items-center gap-1.5 text-xs font-semibold uppercase tracking-[0.18em] text-brand bg-brand/10 border border-brand/20 px-3 py-1 rounded-full">Dashboard</span>
      <h1 class="mt-3 text-3xl sm:text-4xl font-bold tracking-tight">Welcome back</h1>
      <p class="mt-2 text-text-muted max-w-xl">Your mail platform at a glance — domains, users, quota and delivery health.</p>
      <div class="mt-5 flex flex-wrap gap-3">
        <a href="/admin" class="bg-brand hover:bg-indigo-500 text-text-primary font-medium px-5 py-2.5 rounded-xl shadow-lg shadow-brand/20 active:scale-[0.98] transition-all">Open admin console</a>
        <a href="/mail" class="bg-surface/70 border border-divider hover:bg-elevated text-text-muted hover:text-text-primary px-5 py-2.5 rounded-xl transition-colors backdrop-blur">Open webmail</a>
      </div>
    </div>
  </section>

  {#if loading}
    <div class="py-6 flex justify-center"><Spinner /></div>
  {:else}
    <div class="grid grid-cols-2 lg:grid-cols-4 gap-4">
      {#each [
        ["Domains", domains, "Total hosted domains"],
        ["Users", users, "Mail accounts"],
        ["Deferred", deferred, "Queue backlog"],
        ["Quota", (quotaUsed || "0") + " MB", "Used of " + (quotaTotal || "0") + " MB"]
      ] as [label, value, sub]}
        <div class="group relative overflow-hidden rounded-2xl border border-divider bg-surface/70 backdrop-blur p-5 shadow-xl hover:bg-surface transition-all">
          <div class="absolute inset-x-0 top-0 h-px bg-gradient-to-r from-transparent via-brand/60 to-transparent"></div>
          <div class="text-xs font-medium uppercase tracking-wider text-text-muted">{label}</div>
          <div class="mt-2 text-3xl font-bold tabular-nums">{value}</div>
          <div class="mt-1 text-xs text-text-dim">{sub}</div>
        </div>
      {/each}
    </div>
    {#if failed}
      <p class="text-text-muted text-sm">Some widgets are unavailable (admin access required).</p>
    {/if}
  {/if}

  <section class="rounded-2xl border border-divider bg-surface/70 backdrop-blur p-6 shadow-xl">
    <div class="flex items-center justify-between gap-3 flex-wrap mb-4">
      <h2 class="text-lg font-semibold">Recent activity</h2>
      <span class="text-xs text-text-dim">latest audit entries</span>
    </div>
    {#if auditRows.length === 0}
      <EmptyState text="No audit entries (or admin access required)." />
    {:else}
      <div class="space-y-2">
        {#each auditRows as row}
          <div class="flex items-center gap-3 p-3 rounded-xl border border-divider/70 bg-appBg/40 hover:bg-elevated/60 transition">
            <div class="grid place-items-center w-9 h-9 rounded-xl bg-gradient-to-br from-brand to-info text-xs font-bold shrink-0">
              {(row.actor ?? row.user ?? row.email ?? "?").toString().slice(0, 1).toUpperCase()}
            </div>
            <div class="min-w-0 flex-1">
              <div class="text-sm font-medium truncate">{row.action ?? row.event ?? ""} <span class="text-text-muted">· {row.target ?? row.object ?? row.id ?? ""}</span></div>
              <div class="text-xs text-text-dim">{row.actor ?? row.user ?? row.email ?? ""}</div>
            </div>
            <div class="text-xs text-text-dim whitespace-nowrap">{row.time ?? row.created_at ?? row.when ?? ""}</div>
          </div>
        {/each}
      </div>
    {/if}
  </section>
  </div>
</AuthGuard>
