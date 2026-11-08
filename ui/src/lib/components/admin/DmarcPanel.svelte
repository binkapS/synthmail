<script lang="ts">
  import * as Api from "$lib/api";
  import { Card, Spinner, EmptyState, ErrorCard, Field, Input, Textarea, Button } from "$lib/ui";

  let rows = $state<any[]>([]);
  let loading = $state(true);
  let err = $state("");
  let domainFilter = $state("");
  let domain = $state("");
  let report_id = $state("");
  let report_type = $state("aggregate");
  let xml = $state("");
  let ingest_pending = $state(false);

  async function load() {
    loading = true;
    err = "";
    try {
      rows = await Api.listDmarc(domainFilter.trim() || undefined);
    } catch (e: any) { err = e?.message ?? String(e); }
    finally { loading = false; }
  }

  async function ingest() {
    if (ingest_pending) return;
    ingest_pending = true;
    err = "";
    try {
      await Api.createDmarcReport({ domain, report_id, xml, report_type });
      domain = ""; report_id = ""; xml = "";
      await load();
    } catch (e: any) { err = e?.message ?? String(e); }
    finally { ingest_pending = false; }
  }

  $effect(() => { load(); });
</script>

<Card>
  <div class="flex items-center justify-between gap-3 flex-wrap">
    <h2 class="text-xl font-semibold">DMARC</h2>
    <div class="flex gap-2">
      <Input bind:value={domainFilter} placeholder="filter domain" class="w-48" />
      <Button variant="secondary" onclick={load}>Apply</Button>
    </div>
  </div>
  {#if err}<ErrorCard error={err} />{/if}
  {#if loading}
    <Spinner />
  {:else if rows.length === 0}
    <EmptyState text="No DMARC reports." />
  {:else}
    <div class="overflow-x-auto mt-3">
      <table class="w-full text-sm">
        <thead class="text-text-muted border-b border-divider"><tr><th class="text-left py-2">Domain</th><th class="text-left py-2">Report ID</th><th class="text-left py-2">Type</th><th class="text-left py-2">Created</th></tr></thead>
        <tbody>
          {#each rows as row}
            <tr class="border-b border-divider">
              <td class="py-2">{row.domain ?? ""}</td>
              <td class="py-2">{row.report_id ?? ""}</td>
              <td class="py-2">{row.report_type ?? ""}</td>
              <td class="py-2">{row.created_at ?? ""}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}

  <div class="border-t border-divider mt-6 pt-4">
    <h3 class="font-semibold">Ingest report</h3>
    <form class="flex flex-col gap-3 mt-3" onsubmit={(ev) => { ev.preventDefault(); ingest(); }}>
      <div class="flex gap-3 flex-wrap">
        <Field label="Domain" class="flex-1 min-w-[160px]"><Input bind:value={domain} placeholder="example.com" /></Field>
        <Field label="Report ID" class="flex-1 min-w-[160px]"><Input bind:value={report_id} placeholder="id" /></Field>
        <Field label="Type" class="w-40">
          <select bind:value={report_type} class="bg-appBg border border-divider text-text-primary rounded-xl px-4 py-3 base-transition focus:outline-none focus:border-focusRing focus:ring-1 focus:ring-focusRing">
            <option value="aggregate">aggregate</option>
            <option value="failure">failure</option>
          </select>
        </Field>
      </div>
      <Field label="XML"><Textarea bind:value={xml} rows={8} /></Field>
      <Button variant="primary" type="submit" disabled={ingest_pending}>{ingest_pending ? "Ingesting…" : "Ingest"}</Button>
    </form>
  </div>
</Card>
