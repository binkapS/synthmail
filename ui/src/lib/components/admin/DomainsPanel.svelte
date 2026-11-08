<script lang="ts">
  import * as Api from "$lib/api";
  import { Card, Spinner, EmptyState, ErrorCard, Field, Input, Button, PendingButton, ConfirmButton } from "$lib/ui";

  let rows = $state<any[]>([]);
  let loading = $state(true);
  let err = $state("");
  let new_name = $state("");
  let new_desc = $state("");
  let detail_title = $state("");
  let detail = $state("");
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
      detail_title = "DNS records";
      detail = JSON.stringify(r, null, 2);
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
      detail_title = "Verify result";
      detail = JSON.stringify(r, null, 2);
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
      detail_title = "DKIM generated";
      detail = JSON.stringify(r, null, 2);
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

  {#if detail}
    <div class="bg-surface border border-divider rounded-2xl p-4 mt-4">
      <div class="flex items-center justify-between gap-3">
        <h3 class="font-semibold">{detail_title}</h3>
        <Button variant="secondary" onclick={() => (detail = "")}>Close</Button>
      </div>
      <pre class="bg-appBg border border-divider rounded-xl p-3 mt-3 text-xs overflow-x-auto whitespace-pre-wrap">{detail}</pre>
    </div>
  {/if}
</Card>
