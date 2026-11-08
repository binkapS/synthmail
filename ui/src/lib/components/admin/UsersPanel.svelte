<script lang="ts">
  import * as Api from "$lib/api";
  import { Card, Spinner, EmptyState, ErrorCard, Field, Input, Button, PendingButton, ConfirmButton } from "$lib/ui";

  let rows = $state<any[]>([]);
  let loading = $state(true);
  let err = $state("");
  let info = $state("");
  let new_email = $state("");
  let new_password = $state("");
  let new_quota = $state("1024");
  let new_kind = $state("user");
  let reset_for = $state<string | null>(null);
  let reset_pw = $state("");
  let create_pending = $state(false);
  let row_pending = $state(false);
  let refresh_pending = $state(false);

  async function load() {
    loading = true;
    refresh_pending = true;
    err = "";
    info = "";
    try {
      rows = await Api.listUsers();
    } catch (e: any) {
      err = e?.message ?? String(e);
    } finally {
      refresh_pending = false;
      loading = false;
    }
  }

  async function create() {
    if (create_pending) return;
    const email = new_email.trim();
    const password = new_password;
    const quota = parseInt(new_quota || "0", 10) || 0;
    if (!email || !password) { err = "Email and password are required."; return; }
    create_pending = true;
    err = "";
    try {
      await Api.createUser({ email, password, kind: new_kind, quota_mb: quota });
      new_email = "";
      new_password = "";
      await load();
    } catch (e: any) {
      err = e?.message ?? String(e);
    } finally {
      create_pending = false;
    }
  }

  async function toggleActive(row: any) {
    if (row_pending) return;
    row_pending = true;
    err = "";
    try {
      await Api.updateUser(row.id ?? "", { active: !row.active });
      await load();
    } catch (e: any) {
      err = e?.message ?? String(e);
    } finally {
      row_pending = false;
    }
  }

  async function resetPasswordSubmit(id: string) {
    if (row_pending) return;
    if (!reset_pw) return;
    row_pending = true;
    err = "";
    info = "";
    try {
      await Api.resetPassword(id, reset_pw);
      info = "Password reset.";
      reset_for = null;
      reset_pw = "";
    } catch (e: any) {
      err = e?.message ?? String(e);
    } finally {
      row_pending = false;
    }
  }

  async function doDelete(id: string) {
    if (row_pending) return;
    row_pending = true;
    err = "";
    try {
      await Api.deleteUser(id);
      await load();
    } catch (e: any) {
      err = e?.message ?? String(e);
    } finally {
      row_pending = false;
    }
  }

  function pct(used: number, total: number) {
    if (!total) return 0;
    return Math.min(100, (used / total) * 100);
  }

  $effect(() => { load(); });
</script>

<Card>
  <div class="flex items-center justify-between gap-3 flex-wrap">
    <h2 class="text-xl font-semibold">Users</h2>
    <PendingButton label="Refresh" pending={refresh_pending} class="bg-surface border border-divider hover:bg-elevated text-text-muted hover:text-text-primary px-5 py-2.5 rounded-xl transition-colors" onclick={load} />
  </div>
  {#if err}<ErrorCard error={err} />{/if}
  {#if info}<div class="bg-surface border border-divider rounded-xl p-3 text-text-muted text-sm">{info}</div>{/if}
  <form class="flex flex-col gap-4" onsubmit={(ev) => { ev.preventDefault(); create(); }}>
    <div class="flex gap-3 flex-wrap">
      <Field label="Email" class="flex-1 min-w-[200px]">
        <Input bind:value={new_email} placeholder="user@example.com" />
      </Field>
      <Field label="Password" class="flex-1 min-w-[200px]">
        <Input type="password" bind:value={new_password} placeholder="••••••••" />
      </Field>
      <Field label="Quota (MB)" class="w-32">
        <Input bind:value={new_quota} placeholder="1024" />
      </Field>
    </div>
    <Field label="Kind" class="w-40">
      <select bind:value={new_kind} class="bg-appBg border border-divider text-text-primary rounded-xl px-4 py-3 base-transition focus:outline-none focus:border-focusRing focus:ring-1 focus:ring-focusRing">
        <option value="user">user</option>
        <option value="relay">relay</option>
        <option value="admin">admin</option>
      </select>
    </Field>
    <Button variant="primary" type="submit" disabled={create_pending}>{create_pending ? "Creating…" : "Create user"}</Button>
  </form>

  {#if loading}
    <Spinner />
  {:else if rows.length === 0}
    <EmptyState text="No users yet." />
  {:else}
    <div class="overflow-x-auto">
      <table class="w-full text-sm mt-3">
        <thead class="text-text-muted border-b border-divider">
          <tr>
            <th class="text-left py-2">Email</th>
            <th class="text-left py-2">Kind</th>
            <th class="text-left py-2">Quota used</th>
            <th class="text-left py-2">Active</th>
            <th class="text-left py-2">Actions</th>
          </tr>
        </thead>
        <tbody>
          {#each rows as row (row.id ?? row.email)}
            <tr class="border-b border-divider">
              <td class="py-2">{row.email ?? ""}</td>
              <td class="py-2">{row.kind ?? ""}</td>
              <td class="py-2">
                {#if row.total !== undefined || row.quota_mb !== undefined}
                  {Number(row.used_mb ?? 0).toFixed(1)} / {Number(row.quota_mb ?? 0)} MB
                  <progress class="w-full h-2 rounded-full" value={row.used_mb ?? 0} max={row.quota_mb ?? 1}></progress>
                  {pct(Number(row.used_mb ?? 0), Number(row.quota_mb ?? 0)).toFixed(0)}%
                {:else}
                  {row.quota_mb ?? ""} MB
                {/if}
              </td>
              <td class="py-2">
                {#if row.active}
                  <span class="bg-success/10 text-success text-xs font-semibold px-2.5 py-1 rounded-full border border-success/20">active</span>
                {:else}
                  <span class="bg-warning/10 text-warning text-xs font-semibold px-2.5 py-1 rounded-full border border-warning/20">disabled</span>
                {/if}
              </td>
              <td class="py-2">
                <div class="flex flex-wrap gap-2 items-center">
                  <PendingButton label={row.active ? "Disable" : "Enable"} pending={row_pending} class="bg-surface border border-divider hover:bg-elevated text-text-muted hover:text-text-primary px-3 py-1.5 rounded-xl transition-colors text-sm" onclick={() => toggleActive(row)} />
                  <PendingButton label="Reset PW" pending={row_pending} class="bg-surface border border-divider hover:bg-elevated text-text-muted hover:text-text-primary px-3 py-1.5 rounded-xl transition-colors text-sm" onclick={() => (reset_for = row.id ?? null)} />
                  <ConfirmButton label="Delete" confirmLabel="Confirm delete" danger pending={row_pending} onConfirm={() => doDelete(row.id ?? "")} />
                </div>
                {#if reset_for === (row.id ?? null)}
                  <div class="flex gap-2 mt-2">
                    <Input type="password" bind:value={reset_pw} placeholder="new password" class="flex-1" />
                    <Button variant="primary" onclick={() => resetPasswordSubmit(row.id ?? "")}>Save</Button>
                    <Button variant="secondary" onclick={() => { reset_for = null; reset_pw = ""; }}>Cancel</Button>
                  </div>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</Card>
