<script lang="ts">
  import * as Api from "$lib/api";
  import { Card, Spinner, EmptyState } from "$lib/ui";

  let domains = $state(0);
  let users = $state(0);
  let quotaUsed = $state("");
  let quotaTotal = $state("");
  let deferred = $state(0);
  let loading = $state(true);
  let failed = $state(false);

  async function load() {
    loading = true;
    failed = false;
    let anyFail = false;
    try { domains = (await Api.listDomains()).length; } catch { anyFail = true; }
    try { users = (await Api.listUsers()).length; } catch { anyFail = true; }
    try {
      const q: any = await Api.quota();
      quotaUsed = String(q.used_mb ?? "");
      quotaTotal = String(q.quota_mb ?? "");
    } catch { anyFail = true; }
    try {
      const q: any = await Api.queues();
      deferred = Number(q?.queue?.deferred ?? 0);
    } catch { anyFail = true; }
    failed = anyFail;
    loading = false;
  }

  $effect(() => { load(); });
</script>

<Card>
  <h2 class="text-lg font-semibold">Overview</h2>
  {#if loading}
    <Spinner />
  {:else}
    <div class="grid grid-cols-1 md:grid-cols-3 gap-6 p-6 bg-appBg rounded-xl mt-3">
      <div class="bg-surface border border-divider rounded-xl p-5 flex flex-col gap-2"><span class="text-text-muted text-sm">Domains</span><span class="text-2xl font-bold">{domains}</span></div>
      <div class="bg-surface border border-divider rounded-xl p-5 flex flex-col gap-2"><span class="text-text-muted text-sm">Users</span><span class="text-2xl font-bold">{users}</span></div>
      <div class="bg-surface border border-divider rounded-xl p-5 flex flex-col gap-2"><span class="text-text-muted text-sm">Quota (used / total MB)</span><span class="text-2xl font-bold">{quotaUsed} / {quotaTotal}</span></div>
      <div class="bg-surface border border-divider rounded-xl p-5 flex flex-col gap-2"><span class="text-text-muted text-sm">Deferred queue</span><span class="text-2xl font-bold">{deferred}</span></div>
    </div>
    {#if failed}<p class="text-text-muted mt-2 text-sm">Some widgets unavailable (admin access required).</p>{/if}
  {/if}
</Card>
