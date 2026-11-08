<script lang="ts">
  import * as Api from "$lib/api";
  import { Card, Spinner, EmptyState, ErrorCard, Field, Input, Button, ConfirmButton } from "$lib/ui";

  let rows = $state<any[]>([]);
  let loading = $state(true);
  let err = $state("");
  let name = $state("");
  let scopes = $state("");
  let expires = $state("0");
  let create_pending = $state(false);
  let row_pending = $state(false);
  let refresh_pending = $state(false);
  let secret = $state("");

  async function load() {
    loading = true;
    refresh_pending = true;
    err = "";
    try { rows = await Api.tokensList(); } catch (e: any) { err = e?.message ?? String(e); }
    finally { refresh_pending = false; loading = false; }
  }
  async function create() {
    if (create_pending) return;
    create_pending = true;
    err = "";
    secret = "";
    try {
      const sc = scopes.split(",").map((s) => s.trim()).filter(Boolean);
      const days = parseInt(expires || "0", 10) || 0;
      const res: any = await Api.tokensCreate(name.trim(), sc, days);
      secret = res?.token ?? "";
      name = ""; scopes = ""; expires = "0";
      await load();
    } catch (e: any) { err = e?.message ?? String(e); }
    finally { create_pending = false; }
  }
  async function revoke(id: string) {
    if (row_pending) return;
    row_pending = true;
    err = "";
    try { await Api.tokensRevoke(id); await load(); } catch (e: any) { err = e?.message ?? String(e); }
    finally { row_pending = false; }
  }
  $effect(() => { load(); });
</script>

<Card>
  <div class="flex items-center justify-between gap-3 flex-wrap">
    <h2 class="text-xl font-semibold">Tokens</h2>
    <button onclick={load} disabled={refresh_pending} class="bg-surface border border-divider hover:bg-elevated text-text-muted hover:text-text-primary px-5 py-2.5 rounded-xl transition-colors">{refresh_pending ? "Refreshing…" : "Refresh"}</button>
  </div>
  {#if err}<ErrorCard error={err} />{/if}
  {#if secret}
    <div class="bg-appBg border border-divider rounded-xl p-3 mt-3">
      <div class="text-sm text-text-muted">New token (shown once):</div>
      <pre class="text-xs mt-1 whitespace-pre-wrap break-all">{secret}</pre>
    </div>
  {/if}
  <form class="flex flex-col gap-3 mt-3" onsubmit={(ev) => { ev.preventDefault(); create(); }}>
    <div class="flex gap-3 flex-wrap">
      <Field label="Name" class="flex-1 min-w-[160px]"><Input bind:value={name} placeholder="ci-token" /></Field>
      <Field label="Scopes (comma)" class="flex-1 min-w-[160px]"><Input bind:value={scopes} placeholder="admin.read" /></Field>
      <Field label="Expires (days)" class="w-32"><Input bind:value={expires} placeholder="0" /></Field>
    </div>
    <Button variant="primary" type="submit" disabled={create_pending}>{create_pending ? "Creating…" : "Create token"}</Button>
  </form>
  {#if loading}
    <Spinner />
  {:else if rows.length === 0}
    <EmptyState text="No tokens." />
  {:else}
    <div class="overflow-x-auto mt-3">
      <table class="w-full text-sm">
        <thead class="text-text-muted border-b border-divider"><tr><th class="text-left py-2">Name/Prefix</th><th class="text-left py-2">Scopes</th><th class="text-left py-2">Expires</th><th class="text-left py-2">Actions</th></tr></thead>
        <tbody>
          {#each rows as row}
            <tr class="border-b border-divider">
              <td class="py-2">{row.name ?? row.prefix ?? ""}</td>
              <td class="py-2">{(row.scopes ?? []).join(", ")}</td>
              <td class="py-2">{row.expires_at ?? "never"}</td>
              <td class="py-2">
                <ConfirmButton label="Revoke" confirmLabel="Confirm revoke" danger pending={row_pending} onConfirm={() => revoke(row.id ?? "")} />
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</Card>
