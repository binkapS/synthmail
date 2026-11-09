<script lang="ts">
  import * as Api from "$lib/api";
  import { Card, Spinner, EmptyState, ErrorCard, Field, Input, Button, PendingButton, ConfirmButton } from "$lib/ui";

  let rows = $state<any[]>([]);
  let loading = $state(true);
  let err = $state("");
  let new_name = $state("");
  let new_desc = $state("");
  let modal_open = $state(false);
  let modal_kind = $state<"dns" | "verify" | "dkim">("dns");
  let modal_title = $state("");
  let modal_data = $state<any>(null);
  let create_pending = $state(false);
  let row_pending = $state(false);
  let refresh_pending = $state(false);

  async function load() {
    loading = true;
    refresh_pending = true;
    err = "";
    try {
      rows = await Api.listDomains();
    } catch (e: any) {
      err = e?.message ?? String(e);
    } finally {
      refresh_pending = false;
      loading = false;
    }
  }

  async function create() {
    if (create_pending) return;
    const name = new_name.trim();
    if (!name) { err = "Domain name is required."; return; }
    create_pending = true;
    err = "";
    try {
      await Api.createDomain({ domain: name, description: null, catch_all: null });
      new_name = "";
      new_desc = "";
      await load();
    } catch (e: any) {
      err = e?.message ?? String(e);
    } finally {
      create_pending = false;
    }
  }

  async function doDelete(id: string) {
    if (row_pending) return;
    row_pending = true;
    err = "";
    try {
      await Api.deleteDomain(id);
      await load();
    } catch (e: any) {
      err = e?.message ?? String(e);
    } finally {
      row_pending = false;
    }
  }

  async function showDns(id: string) {
    if (row_pending) return;
    row_pending = true;
    err = "";
    try {
      const r = await Api.domainDns(id);
      modal_kind = "dns";
      modal_title = "DNS setup";
      modal_data = r;
      modal_open = true;
    } catch (e: any) {
      err = e?.message ?? String(e);
    } finally {
      row_pending = false;
    }
  }

  async function showVerify(id: string) {
    if (row_pending) return;
    row_pending = true;
    err = "";
    try {
      const r = await Api.domainVerify(id);
      modal_kind = "verify";
      modal_title = "DNS verification";
      modal_data = r;
      modal_open = true;
    } catch (e: any) {
      err = e?.message ?? String(e);
    } finally {
      row_pending = false;
    }
  }

  async function showDkim(id: string) {
    if (row_pending) return;
    row_pending = true;
    err = "";
    try {
      const r = await Api.dkimGenerate(id, { key_type: null, selector: null });
      modal_kind = "dkim";
      modal_title = "DKIM generated";
      modal_data = r;
      modal_open = true;
    } catch (e: any) {
      err = e?.message ?? String(e);
    } finally {
      row_pending = false;
    }
  }

  $effect(() => { load(); });
</script>

<Card>
  <div class="flex items-center justify-between gap-3 flex-wrap">
    <h2 class="text-xl font-semibold">Domains</h2>
    <PendingButton label="Refresh" pending={refresh_pending} class="bg-surface border border-divider hover:bg-elevated text-text-muted hover:text-text-primary px-5 py-2.5 rounded-xl transition-colors" onclick={load} />
  </div>
  {#if err}<ErrorCard error={err} />{/if}
  <form class="flex flex-col gap-4" onsubmit={(ev) => { ev.preventDefault(); create(); }}>
    <div class="flex gap-3 flex-wrap">
      <Field label="Domain" class="flex-1 min-w-[200px]">
        <Input bind:value={new_name} placeholder="example.com" />
      </Field>
      <Field label="Description" class="flex-1 min-w-[200px]">
        <Input bind:value={new_desc} placeholder="" />
      </Field>
    </div>
    <Button variant="primary" type="submit" disabled={create_pending}>{create_pending ? "Adding…" : "Add domain"}</Button>
  </form>

  {#if loading}
    <Spinner />
  {:else if rows.length === 0}
    <EmptyState text="No domains yet." />
  {:else}
    <div class="overflow-x-auto">
      <table class="w-full text-sm mt-3">
        <thead class="text-text-muted border-b border-divider">
          <tr>
            <th class="text-left py-2">Domain</th>
            <th class="text-left py-2">DKIM</th>
            <th class="text-left py-2">Active</th>
            <th class="text-left py-2">Actions</th>
          </tr>
        </thead>
        <tbody>
          {#each rows as row (row.id ?? row.domain)}
            <tr class="border-b border-divider">
              <td class="py-2">{row.domain ?? row.name ?? ""}</td>
              <td class="py-2">
                {#if row.dkim_enabled}
                  <span class="bg-success/10 text-success text-xs font-semibold px-2.5 py-1 rounded-full border border-success/20">enabled</span>
                {:else}
                  —
                {/if}
              </td>
              <td class="py-2">{row.active ? "active" : "inactive"}</td>
              <td class="py-2">
                <div class="flex flex-wrap gap-2">
                  <PendingButton label="DNS" pending={row_pending} class="bg-surface border border-divider hover:bg-elevated text-text-muted hover:text-text-primary px-3 py-1.5 rounded-xl transition-colors text-sm" onclick={() => showDns(row.id ?? "")} />
                  <PendingButton label="Verify" pending={row_pending} class="bg-surface border border-divider hover:bg-elevated text-text-muted hover:text-text-primary px-3 py-1.5 rounded-xl transition-colors text-sm" onclick={() => showVerify(row.id ?? "")} />
                  <PendingButton label="DKIM" pending={row_pending} class="bg-surface border border-divider hover:bg-elevated text-text-muted hover:text-text-primary px-3 py-1.5 rounded-xl transition-colors text-sm" onclick={() => showDkim(row.id ?? "")} />
                  <ConfirmButton label="Delete" confirmLabel="Confirm delete" danger pending={row_pending} onConfirm={() => doDelete(row.id ?? "")} />
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}

  {#if modal_open && modal_data}
    <div class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-sm" onclick={(e) => { if (e.target === e.currentTarget) modal_open = false; }} onkeydown={(e) => { if (e.key === 'Escape') modal_open = false; }} role="presentation">
      <div class="bg-surface border border-divider rounded-3xl p-6 max-w-3xl w-full max-h-[85vh] overflow-y-auto shadow-2xl">
        <div class="flex items-center justify-between gap-3 mb-4">
          <h3 class="text-lg font-semibold">{modal_title}</h3>
          <Button variant="secondary" onclick={() => (modal_open = false)}>Close</Button>
        </div>

        {#if modal_kind === "dns"}
          <p class="text-text-muted text-sm mb-4">Copy each record into your DNS provider. Changes can take a few minutes to propagate.</p>
          <div class="grid gap-3">
            {#each Object.keys(modal_data) as key}
              {#if key !== "domain" && key !== "mail_host"}
                <div class="rounded-xl border border-divider bg-appBg/40 p-4">
                  <div class="text-xs uppercase tracking-wider text-text-muted mb-1">{key.replace(/_/g, " ")}</div>
                  <div class="font-mono text-sm break-all">{modal_data[key]}</div>
                </div>
              {/if}
            {/each}
            <div class="rounded-xl border border-divider bg-appBg/40 p-4">
              <div class="text-xs uppercase tracking-wider text-text-muted mb-1">Domain</div>
              <div class="font-mono text-sm">{modal_data.domain}</div>
            </div>
            {#if modal_data.mail_host}
              <div class="rounded-xl border border-divider bg-appBg/40 p-4">
                <div class="text-xs uppercase tracking-wider text-text-muted mb-1">Mail host</div>
                <div class="font-mono text-sm">{modal_data.mail_host}</div>
              </div>
            {/if}
          </div>
          <div class="mt-4 text-xs text-text-dim">Tip: after adding records, run “Verify” to check them automatically.</div>

        {:else if modal_kind === "verify"}
          <div class="mb-4">
            <span class="px-3 py-1 rounded-full text-xs font-semibold border {modal_data.deliverable ? 'bg-success/10 text-success border-success/20' : 'bg-danger/10 text-danger border-danger/20'}">
              {modal_data.deliverable ? "Deliverable" : "Not ready"}
            </span>
            {#if modal_data.score_hint}
              <span class="text-text-muted text-sm ml-2">{modal_data.score_hint}</span>
            {/if}
          </div>
          <div class="grid gap-2">
            {#each Object.entries(modal_data.checks ?? {}) as [name, raw]}
              {@const info = raw as any}
              <div class="flex items-start gap-3 rounded-xl border border-divider bg-appBg/40 p-3">
                <span class="text-lg leading-none">{info?.ok ? "✅" : "❌"}</span>
                <div class="min-w-0 flex-1">
                  <div class="text-sm font-medium capitalize">{name}</div>
                  {#if info?.records?.length}
                    <div class="text-xs text-text-muted break-all">{info.records.join(", ")}</div>
                  {:else}
                    <div class="text-xs text-text-muted">No records found</div>
                  {/if}
                  {#if info?.selector}
                    <div class="text-xs text-text-dim">selector: {info.selector}</div>
                  {/if}
                </div>
              </div>
            {/each}
          </div>

        {:else}
          <pre class="bg-appBg border border-divider rounded-xl p-3 text-xs overflow-x-auto whitespace-pre-wrap">{JSON.stringify(modal_data, null, 2)}</pre>
        {/if}
      </div>
    </div>
  {/if}
</Card>
