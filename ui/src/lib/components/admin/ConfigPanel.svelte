<script lang="ts">
  import * as Api from "$lib/api";
  import { Card, Spinner, Field, Textarea, Button } from "$lib/ui";

  let exported = $state("");
  let importBody = $state("");
  let loading = $state(true);
  let err = $state("");
  let info = $state("");
  let import_pending = $state(false);

  async function load() {
    loading = true;
    err = "";
    try {
      exported = await Api.configExport();
    } catch (e: any) { err = e?.message ?? String(e); }
    finally { loading = false; }
  }

  async function doImport() {
    if (import_pending) return;
    import_pending = true;
    err = ""; info = "";
    try {
      let parsed: any;
      try { parsed = JSON.parse(importBody); } catch { err = "Invalid JSON"; return; }
      const res: any = await Api.configImport(parsed);
      info = `Imported: ${JSON.stringify(res)}`;
    } catch (e: any) { err = e?.message ?? String(e); }
    finally { import_pending = false; }
  }

  $effect(() => { load(); });
</script>

<Card>
  <h2 class="text-xl font-semibold">Config</h2>
  {#if err}<p class="text-danger text-sm">{err}</p>{/if}
  {#if info}<p class="text-success text-sm">{info}</p>{/if}
  {#if loading}
    <Spinner />
  {:else}
    <Field label="Exported config">
      <Textarea bind:value={exported} rows={12} />
    </Field>
  {/if}
  <div class="border-t border-divider mt-4 pt-4">
    <h3 class="font-semibold">Import config</h3>
    <Field label="Config JSON" class="mt-2">
      <Textarea bind:value={importBody} rows={8} />
    </Field>
    <div class="mt-3">
      <Button variant="primary" onclick={doImport} disabled={import_pending}>{import_pending ? "Importing…" : "Import config"}</Button>
    </div>
  </div>
</Card>
