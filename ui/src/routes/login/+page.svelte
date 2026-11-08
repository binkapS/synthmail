<script lang="ts">
  import { goto } from "$app/navigation";
  import * as Api from "$lib/api";
  import GuestGuard from "$lib/components/GuestGuard.svelte";
  import { setAuthToken } from "$lib/stores/auth.svelte";
  import { Card, Field, Input, Button, ErrorCard, Spinner } from "$lib/ui";

  let email = $state("");
  let password = $state("");
  let totp = $state("");
  let showTotp = $state(false);
  let loading = $state(false);
  let err = $state("");

  async function doLogin() {
    const e = email.trim();
    if (!e || !password) {
      err = "Enter email and password.";
      return;
    }
    err = "";
    loading = true;
    try {
      const resp: any = await Api.login(e, password, totp.trim() || undefined);
      if (resp.token) setAuthToken(resp.token);
      const kind = resp.kind;
      if (kind === "admin" || kind === "superadmin") goto("/admin");
      else goto("/mail");
    } catch (ex: any) {
      err = ex?.message ?? String(ex);
    } finally {
      loading = false;
    }
  }
</script>

<GuestGuard>
  <div class="grid place-items-center min-h-[60vh] p-4">
    <Card class="w-full max-w-md">
      <div class="mb-4">
        <h2 class="text-xl font-semibold">Sign in</h2>
        <p class="text-text-muted text-sm">Access admin & webmail with your mail account (TOTP supported).</p>
      </div>
      <form class="flex flex-col gap-4" onsubmit={(ev) => { ev.preventDefault(); doLogin(); }}>
        <Field label="Email">
          <Input type="email" bind:value={email} placeholder="you@example.com" />
        </Field>
        <Field label="Password">
          <Input type="password" bind:value={password} placeholder="••••••••" />
        </Field>
        {#if showTotp}
          <Field label="TOTP code">
            <Input type="text" bind:value={totp} placeholder="123456" />
          </Field>
        {/if}
        <div class="flex items-center justify-between gap-3 flex-wrap">
          <Button variant="secondary" type="button" onclick={() => (showTotp = !showTotp)}>
            {showTotp ? "Hide TOTP" : "Add TOTP code"}
          </Button>
          <button
            type="submit"
            disabled={loading}
            class="bg-brand hover:bg-indigo-500 text-text-primary font-medium px-5 py-2.5 rounded-xl shadow-lg shadow-brand/20 active:scale-[0.98] transition-all disabled:opacity-50"
          >
            {loading ? "Signing in…" : "Sign in"}
          </button>
        </div>
      </form>
      {#if err}
        <div class="mt-4"><ErrorCard error={err} /></div>
      {/if}
      {#if loading}
        <div class="mt-4"><Spinner /></div>
      {/if}
    </Card>
  </div>
</GuestGuard>
