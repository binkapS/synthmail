<script lang="ts">
  import * as Api from "$lib/api";
  import { Card, Spinner, EmptyState, ErrorCard, Field, Input, Button, PendingButton, ConfirmButton } from "$lib/ui";

  let rows = $state<any[]>([]);
  let loading = $state(true);
  let err = $state("");
  let addr = $state("");
  let goto = $state("");
  let create_pending = $state(false);
  let row_pending = $state(false);
  let refresh_pending = $state(false);

  async function load() {
    loading = true;
    refresh_pending = true;
    err = "";
    try { rows = await Api.listAliases(); } catch (e: any) { err = e?.message ?? String(e); }
    finally { refresh_pending = false; loading = false; }
  }
  async function create() {
    if (create_pending) return;
    const a = addr.trim();
    const g = goto.trim();
    if (!a || !g) { err = "Both address and goto are required."; return; }
    create_pending = true;
    err = "";
    try {
      await Api.createAlias({ address: a, goto: g });
      addr = ""; goto = "";
      await load();
    } catch (e: any) { err = e?.message ?? String(e); }
    finally { create_pending = false; }
  }
  async function doDelete(id: string) {
    if (row_pending) return;
    row_pending = true;
    err = "";
    try { await Api.deleteAlias(id); await load(); } catch (e: any) { err = e?.message ?? String(e); }
    finally { row_pending = false; }
  }
  $effect(() => { load(); });
</script>

<Card>
  <div class="flex items-center justify-between gap-3 flex-wrap">
    <h2 class="text-xl font-semibold">Aliases</h2>
    <PendingButton label="Refresh" pending={refresh_pending} class="bg-surface border border-divider hover:bg-elevated text-text-muted hover:text-text-primary px-5 py-2.5 rounded-xl transition-colors" onclick={load} />
  </div>
  {#if err}<ErrorCard error={err} />{/if}
  <form class="flex flex-col gap-4" onsubmit={(ev) => { ev.preventDefault(); create(); }}>
    <div class="flex gap-3 flex-wrap">
      <Field label="Address" class="flex-1 min-w-[200px]">
        <Input bind:value={addr} placeholder="alias@example.com" />
      </Field>
      <Field label="Goto (comma separated)" class="flex-1 min-w-[200px]">
        <Input bind:value={goto} placeholder="a@x, b@x" />
      </Field>
    </div>
    <Button variant="primary" type="submit" disabled={create_pending}>{create_pending ? "Creating…" : "Create alias"}</Button>
  </form>
  {#if loading}
    <Spinner />
  {:else if rows.length === 0}
    <EmptyState text="No aliases yet." />
  {:else}
    <div class="overflow-x-auto">
      <table class="w-full text-sm mt-3">
        <thead class="text-text-muted border-b border-divider"><tr><th class="text-left py-2">Address</th><th class="text-left py-2">Goto</th><th class="text-left py-2">Active</th><th class="text-left py-2">Actions</th></tr></thead>
        <tbody>
          {#each rows as row (row.id ?? row.address)}
            <tr class="border-b border-divider">
              <td class="py-2">{row.address ?? ""}</td>
              <td class="py-2">{row.goto ?? ""}</td>
              <td class="py-2">{row.active ? "active" : "inactive"}</td>
              <td class="py-2">
                <ConfirmButton label="Delete" confirmLabel="Confirm delete" danger pending={row_pending} onConfirm={() => doDelete(row.id ?? "")} />
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</Card>
