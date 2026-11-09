<script lang="ts">
  import * as Api from "$lib/api";
  import { Card, Spinner, Field, Input } from "$lib/ui";

  let me = $state<any>(null);
  let health = $state("unknown");
  let queues = $state<any>(null);
  let quota = $state<any>(null);
  let loading = $state(true);
  let err = $state("");

  async function load() {
    loading = true;
    err = "";
    try { me = await Api.me(); } catch (e: any) { err = e?.message ?? String(e); }
    try {
      const r = await fetch("/healthz");
      health = r.ok ? "healthy" : `http ${r.status}`;
    } catch {
      health = "unreachable";
    }
    try { queues = await Api.queues(); } catch { /* noop */ }
    try { quota = await Api.quota(); } catch { /* noop */ }
    loading = false;
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
  {:else}
    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 mt-3">
      <div class="bg-appBg border border-divider rounded-xl p-4">
        <div class="text-xs text-text-muted uppercase tracking-wider">Identity</div>
        <div class="mt-1 text-sm text-text-primary">{me?.email ?? "—"}</div>
        <div class="text-xs text-text-dim">{me?.kind ?? ""} · {me?.id ?? ""}</div>
      </div>
      <div class="bg-appBg border border-divider rounded-xl p-4">
        <div class="text-xs text-text-muted uppercase tracking-wider">API base</div>
        <div class="mt-1 text-sm text-text-primary font-mono">/api/v1</div>
        <div class="text-xs text-text-dim">Webmail health: {health}</div>
      </div>
      <div class="bg-appBg border border-divider rounded-xl p-4">
        <div class="text-xs text-text-muted uppercase tracking-wider">Force password change</div>
        <div class="mt-1 text-sm text-text-primary">{me?.force_password_change ? "yes" : "no"}</div>
        <div class="text-xs text-text-dim">Quota: {me?.quota_mb ?? 0} MB</div>
      </div>
      {#if queues}
        <div class="bg-appBg border border-divider rounded-xl p-4">
          <div class="text-xs text-text-muted uppercase tracking-wider">Queue</div>
          <div class="mt-1 text-sm text-text-primary">deferred {queues.queue?.deferred ?? 0} · active {queues.queue?.active ?? 0} · hold {queues.queue?.hold ?? 0}</div>
          <div class="text-xs text-text-dim">activity 1h {queues.activity_1h ?? 0} · postfix {queues.postfix_queue_size ?? 0}</div>
        </div>
      {/if}
      {#if quota}
        <div class="bg-appBg border border-divider rounded-xl p-4">
          <div class="text-xs text-text-muted uppercase tracking-wider">Quota totals</div>
          <div class="mt-1 text-sm text-text-primary">{quota.used_mb ?? 0} / {quota.quota_mb ?? 0} MB</div>
          <div class="text-xs text-text-dim">{(quota.users ?? []).length} users with quota rows</div>
        </div>
      {/if}
    </div>
    <div class="mt-4">
      <Field label="User agent">
        <Input value={typeof navigator !== 'undefined' ? navigator.userAgent : ''} readonly disabled />
      </Field>
    </div>
  {/if}
</Card>
