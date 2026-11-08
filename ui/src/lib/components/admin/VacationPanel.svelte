<script lang="ts">
  import * as Api from "$lib/api";
  import { Card, Spinner, Field, Input, Textarea, Button } from "$lib/ui";

  let users = $state<any[]>([]);
  let selected = $state("");
  let subject = $state("");
  let message = $state("");
  let enabled = $state(false);
  let start_date = $state("");
  let end_date = $state("");
  let loading = $state(true);
  let err = $state("");
  let save_pending = $state(false);

  async function loadUsers() {
    loading = true;
    err = "";
    try {
      users = await Api.listUsers();
      if (!selected && users.length) selected = users[0].id ?? users[0].email ?? "";
      await loadVac();
    } catch (e: any) { err = e?.message ?? String(e); }
    finally { loading = false; }
  }

  async function loadVac() {
    if (!selected) return;
    err = "";
    try {
      const r: any = await Api.vacationGet(selected);
      subject = r?.subject ?? "";
      message = r?.message ?? "";
      enabled = !!r?.enabled;
      start_date = r?.start_date ?? "";
      end_date = r?.end_date ?? "";
    } catch (e: any) { err = e?.message ?? String(e); }
  }

  async function save() {
    if (save_pending) return;
    save_pending = true;
    err = "";
    try {
      await Api.vacationPut(selected, { subject, message, enabled, start_date: start_date || null, end_date: end_date || null });
    } catch (e: any) { err = e?.message ?? String(e); }
    finally { save_pending = false; }
  }

  $effect(() => { loadUsers(); });
</script>

<Card>
  <h2 class="text-xl font-semibold">Vacation</h2>
  {#if err}<p class="text-danger text-sm">{err}</p>{/if}
  {#if loading}
    <Spinner />
  {:else}
    <div class="flex gap-3 flex-wrap items-end mt-3">
      <Field label="User" class="flex-1 min-w-[200px]">
        <select bind:value={selected} onchange={loadVac} class="bg-appBg border border-divider text-text-primary rounded-xl px-4 py-3 base-transition focus:outline-none focus:border-focusRing focus:ring-1 focus:ring-focusRing">
          {#each users as u}
            <option value={u.id ?? u.email}>{u.email ?? u.id}</option>
          {/each}
        </select>
      </Field>
    </div>
    <div class="flex flex-col gap-3 mt-3">
      <Field label="Subject"><Input bind:value={subject} placeholder="Out of office" /></Field>
      <Field label="Message"><Textarea bind:value={message} rows={6} /></Field>
      <div class="flex items-center gap-3">
        <input type="checkbox" bind:checked={enabled} class="accent-brand" />
        <span class="text-text-muted text-sm">Enabled</span>
      </div>
      <div class="flex gap-3 flex-wrap">
        <Field label="Start date" class="flex-1"><Input bind:value={start_date} placeholder="2026-01-01" /></Field>
        <Field label="End date" class="flex-1"><Input bind:value={end_date} placeholder="2026-01-10" /></Field>
      </div>
      <Button variant="primary" onclick={save} disabled={save_pending}>{save_pending ? "Saving…" : "Save vacation"}</Button>
    </div>
  {/if}
</Card>
