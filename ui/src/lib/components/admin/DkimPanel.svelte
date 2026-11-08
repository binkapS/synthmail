<script lang="ts">
  import * as Api from "$lib/api";
  import { Card, Spinner, EmptyState, Field, Input, Button } from "$lib/ui";

  let domains = $state<any[]>([]);
  let selected = $state("");
  let keys = $state<any[]>([]);
  let genResult = $state<any>(null);
  let loading = $state(true);
  let err = $state("");

  async function loadDomains() {
    loading = true;
    err = "";
    try {
      domains = await Api.listDomains();
      if (!selected && domains.length) selected = domains[0].id ?? "";
      await loadKeys();
    } catch (e: any) {
      err = e?.message ?? String(e);
    } finally {
      loading = false;
    }
  }

  async function loadKeys() {
    if (!selected) return;
    err = "";
    try {
      keys = await Api.dkimList(selected);
    } catch (e: any) {
      err = e?.message ?? String(e);
    }
  }

  async function generate() {
    if (!selected) return;
    err = "";
    try {
      genResult = await Api.dkimGenerate(selected, { key_type: null, selector: null });
      await loadKeys();
    } catch (e: any) {
      err = e?.message ?? String(e);
    }
  }

  $effect(() => { loadDomains(); });
</script>

<Card>
  <h2 class="text-xl font-semibold">DKIM</h2>
  {#if err}<p class="text-danger text-sm">{err}</p>{/if}
  {#if loading}
    <Spinner />
  {:else}
    <div class="flex gap-3 flex-wrap items-end mt-3">
      <Field label="Domain" class="flex-1 min-w-[200px]">
        <select bind:value={selected} onchange={loadKeys} class="bg-appBg border border-divider text-text-primary rounded-xl px-4 py-3 base-transition focus:outline-none focus:border-focusRing focus:ring-1 focus:ring-focusRing">
          {#each domains as d}
            <option value={d.id}>{d.domain ?? d.id}</option>
          {/each}
        </select>
      </Field>
      <Button variant="primary" onclick={generate}>Generate</Button>
      <Button variant="secondary" onclick={loadKeys}>Refresh</Button>
    </div>

    {#if genResult}
      <div class="bg-appBg border border-divider rounded-xl p-4 mt-3">
        <div class="text-sm text-text-muted">Selector: {genResult.selector}</div>
        <pre class="text-xs mt-2 whitespace-pre-wrap overflow-x-auto">{genResult.dns_record ?? JSON.stringify(genResult, null, 2)}</pre>
      </div>
    {/if}

    {#if keys.length === 0}
      <EmptyState text="No keys yet." />
    {:else}
      <div class="overflow-x-auto mt-3">
        <table class="w-full text-sm">
          <thead class="text-text-muted border-b border-divider"><tr><th class="text-left py-2">Selector</th><th class="text-left py-2">Revoked</th><th class="text-left py-2">Public key</th></tr></thead>
          <tbody>
            {#each keys as k}
              <tr class="border-b border-divider">
                <td class="py-2">{k.selector ?? ""}</td>
                <td class="py-2">{k.revoked ? "revoked" : "active"}</td>
                <td class="py-2 truncate max-w-[200px]">{k.public_key ?? ""}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  {/if}
</Card>
