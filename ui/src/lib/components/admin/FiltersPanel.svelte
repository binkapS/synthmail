<script lang="ts">
  import * as Api from "$lib/api";
  import { Card, Spinner, EmptyState, Field, Textarea, Button } from "$lib/ui";

  let users = $state<any[]>([]);
  let selected = $state("");
  let sieve = $state("");
  let loading = $state(true);
  let err = $state("");
  let save_pending = $state(false);

  async function loadUsers() {
    loading = true;
    err = "";
    try {
      users = await Api.listUsers();
      if (!selected && users.length) selected = users[0].id ?? users[0].email ?? "";
      await loadFilter();
    } catch (e: any) {
      err = e?.message ?? String(e);
    } finally {
      loading = false;
    }
  }

  async function loadFilter() {
    if (!selected) return;
    err = "";
    try {
      const r: any = await Api.getFilters(selected);
      if (typeof r === "string") sieve = r;
      else if (r?.sieve) sieve = r.sieve;
      else if (Array.isArray(r?.scripts)) sieve = r.scripts.map((s: any) => s.content ?? "").join("\n\n");
      else if (Array.isArray(r)) sieve = r.map((s: any) => s.content ?? JSON.stringify(s)).join("\n\n");
      else sieve = JSON.stringify(r, null, 2);
    } catch (e: any) {
      err = e?.message ?? String(e);
    }
  }

  async function save() {
    if (save_pending) return;
    save_pending = true;
    err = "";
    try {
      await Api.filtersPut(selected, sieve);
    } catch (e: any) {
      err = e?.message ?? String(e);
    } finally {
      save_pending = false;
    }
  }

  $effect(() => { loadUsers(); });
</script>

<Card>
  <h2 class="text-xl font-semibold">Filters</h2>
  {#if err}<p class="text-danger text-sm">{err}</p>{/if}
  {#if loading}
    <Spinner />
  {:else}
    <div class="flex gap-3 flex-wrap items-end mt-3">
      <Field label="User" class="flex-1 min-w-[200px]">
        <select bind:value={selected} onchange={loadFilter} class="bg-appBg border border-divider text-text-primary rounded-xl px-4 py-3 base-transition focus:outline-none focus:border-focusRing focus:ring-1 focus:ring-focusRing">
          {#each users as u}
            <option value={u.id ?? u.email}>{u.email ?? u.id}</option>
          {/each}
        </select>
      </Field>
      <Button variant="primary" onclick={save} disabled={save_pending}>{save_pending ? "Saving…" : "Save filters"}</Button>
    </div>
    <div class="mt-3">
      <Field label="Sieve">
        <Textarea bind:value={sieve} rows={12} />
      </Field>
    </div>
  {/if}
</Card>
