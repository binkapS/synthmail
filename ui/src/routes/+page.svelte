<script lang="ts">
  import { goto } from "$app/navigation";
  import * as Api from "$lib/api";
  import AuthGuard from "$lib/components/AuthGuard.svelte";
  import { Card, Spinner, EmptyState } from "$lib/ui";
  import { setAuthToken } from "$lib/stores/auth.svelte";

  let domains = $state(0);
  let users = $state(0);
  let quotaUsed = $state("");
  let quotaTotal = $state("");
  let deferred = $state(0);
  let loading = $state(true);
  let failed = $state(false);
  let auditRows = $state<any[]>([]);

  $effect(() => {
    if (!Api.getToken()) {
      goto("/login");
      return;
    }
    let cancelled = false;
    (async () => {
      let anyFail = false;
      try {
        const r = await Api.listDomains();
        if (!cancelled) domains = r.length;
      } catch {
        anyFail = true;
      }
      try {
        const r = await Api.listUsers();
        if (!cancelled) users = r.length;
      } catch {
        anyFail = true;
      }
      try {
        const r: any = await Api.quota();
        if (!cancelled) {
          quotaUsed = String(r.used_mb ?? "");
          quotaTotal = String(r.quota_mb ?? "");
        }
      } catch {
        anyFail = true;
      }
      try {
        const r: any = await Api.queues();
        if (!cancelled) deferred = Number(r?.queue?.deferred ?? 0);
      } catch {
        anyFail = true;
      }
      try {
        const r = await Api.audit(5);
        if (!cancelled) auditRows = r.slice(0, 5);
      } catch {
        /* ok */
      }
      if (!cancelled) {
        failed = anyFail;
        loading = false;
      }
    })();
    return () => {
      cancelled = true;
    };
  });
</script>

<AuthGuard>
  <section class="bg-surface border border-divider rounded-2xl p-6 shadow-xl">
    <h1 class="text-2xl font-bold">Dashboard</h1>
    <p class="text-text-muted">Mail platform overview.</p>
  </section>

  {#if loading}
    <div class="mt-4"><Spinner /></div>
  {:else}
    <div class="grid grid-cols-1 md:grid-cols-3 gap-6 p-6 bg-appBg min-h-0 text-text-primary mt-4">
      <div class="bg-surface border border-divider rounded-xl p-5 flex flex-col gap-2">
        <span class="text-text-muted text-sm">Domains</span>
        <span class="text-2xl font-bold">{domains}</span>
      </div>
      <div class="bg-surface border border-divider rounded-xl p-5 flex flex-col gap-2">
        <span class="text-text-muted text-sm">Users</span>
        <span class="text-2xl font-bold">{users}</span>
      </div>
      <div class="bg-surface border border-divider rounded-xl p-5 flex flex-col gap-2">
        <span class="text-text-muted text-sm">Quota (used / total MB)</span>
        <span class="text-2xl font-bold">{quotaUsed} / {quotaTotal}</span>
      </div>
      <div class="bg-surface border border-divider rounded-xl p-5 flex flex-col gap-2">
        <span class="text-text-muted text-sm">Deferred queue</span>
        <span class="text-2xl font-bold">{deferred}</span>
      </div>
    </div>
    {#if failed}
      <p class="text-text-muted mt-2">Some widgets are unavailable (admin access required).</p>
    {/if}
  {/if}

  <Card class="mt-4">
    <h2 class="text-lg font-semibold">Quick links</h2>
    <div class="flex flex-wrap gap-3 mt-3">
      <a href="/admin" class="bg-brand hover:bg-indigo-500 text-text-primary font-medium px-5 py-2.5 rounded-xl shadow-lg shadow-brand/20 active:scale-[0.98] transition-all">Admin (domains, users, DKIM…)</a>
      <a href="/admin" class="bg-surface border border-divider hover:bg-elevated text-text-muted hover:text-text-primary px-5 py-2.5 rounded-xl transition-colors">Config & tokens</a>
      <a href="/mail" class="bg-surface border border-divider hover:bg-elevated text-text-muted hover:text-text-primary px-5 py-2.5 rounded-xl transition-colors">Webmail</a>
      <a href="/login" class="bg-surface border border-divider hover:bg-elevated text-text-muted hover:text-text-primary px-5 py-2.5 rounded-xl transition-colors">Sign in</a>
    </div>
  </Card>

  <Card class="mt-4">
    <h2 class="text-lg font-semibold">Recent audit</h2>
    {#if auditRows.length === 0}
      <EmptyState text="No audit entries (or admin access required)." />
    {:else}
      <div class="overflow-x-auto mt-3">
        <table class="w-full text-sm">
          <thead>
            <tr class="text-left text-text-muted border-b border-divider">
              <th class="py-2">Actor</th>
              <th class="py-2">Action</th>
              <th class="py-2">Target</th>
              <th class="py-2">Time</th>
            </tr>
          </thead>
          <tbody>
            {#each auditRows as row}
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
</AuthGuard>
