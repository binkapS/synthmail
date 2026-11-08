<script lang="ts">
  import { goto } from "$app/navigation";
  import * as Api from "$lib/api";
  import { clearAuth } from "$lib/stores/auth.svelte";
  import { Card, Spinner, EmptyState, ErrorCard, Field, Input, Button } from "$lib/ui";

  let me = $state<any>(null);
  let forced = $state(false);
  let loading = $state(true);
  let err = $state("");
  let info = $state("");

  // password change
  let curPw = $state("");
  let newPw = $state("");
  let pw_pending = $state(false);

  // totp
  let totpPw = $state("");
  let totpCode = $state("");
  let enrollSecret = $state("");
  let enrollUrl = $state("");
  let enroll_pending = $state(false);
  let verify_pending = $state(false);
  let disable_pending = $state(false);

  async function load() {
    loading = true;
    err = "";
    try {
      me = await Api.me();
      forced = !!me?.force_password_change;
    } catch (e: any) { err = e?.message ?? String(e); }
    finally { loading = false; }
  }

  async function changePw() {
    if (pw_pending) return;
    pw_pending = true;
    err = ""; info = "";
    try {
      await Api.changePassword(curPw, newPw);
      forced = false;
      curPw = "";
      newPw = "";
      // The backend increments token_version on password change, so the
      // current session is now revoked: clear it and require re-login.
      Api.clearToken();
      clearAuth();
      info = "Password changed. Please sign in again.";
      setTimeout(() => goto("/login"), 800);
    } catch (e: any) {
      const m = e?.message ?? String(e);
      if (m.includes("HTTP 401")) err = "Current password is incorrect (HTTP 401).";
      else err = m;
    }
    finally { pw_pending = false; }
  }

  async function enroll() {
    if (enroll_pending) return;
    enroll_pending = true;
    err = ""; info = "";
    try {
      const r: any = await Api.totpEnroll(totpPw, totpCode || undefined);
      enrollSecret = r?.secret ?? "";
      enrollUrl = r?.otpauth_url ?? "";
    } catch (e: any) { err = e?.message ?? String(e); }
    finally { enroll_pending = false; }
  }

  async function verify() {
    if (verify_pending) return;
    verify_pending = true;
    err = ""; info = "";
    try {
      await Api.totpVerify(totpCode);
      info = "2FA enabled.";
      enrollSecret = ""; enrollUrl = "";
    } catch (e: any) { err = e?.message ?? String(e); }
    finally { verify_pending = false; }
  }

  async function disable2fa() {
    if (disable_pending) return;
    disable_pending = true;
    err = ""; info = "";
    try {
      await Api.totpDisable(totpPw);
      info = "2FA disabled.";
    } catch (e: any) { err = e?.message ?? String(e); }
    finally { disable_pending = false; }
  }

  $effect(() => { load(); });
</script>

<Card>
  <h2 class="text-xl font-semibold">Account</h2>
  {#if loading}
    <Spinner />
  {:else if me}
    <p class="text-text-muted text-sm">Signed in as {me.email ?? ""} ({me.kind ?? ""})</p>
    {#if err}<ErrorCard error={err} />{/if}
    {#if info}<p class="text-success text-sm">{info}</p>{/if}

    <div class="border-t border-divider mt-4 pt-4">
      <h3 class="font-semibold">Change password</h3>
      {#if forced}
        <p class="text-warning text-sm mt-1">Your password must be changed before using webmail. Enter your current password below.</p>
      {/if}
      <div class="flex flex-col gap-3 mt-3 max-w-md">
        <Field label="Current password"><Input type="password" bind:value={curPw} /></Field>
        <Field label="New password"><Input type="password" bind:value={newPw} /></Field>
        <Button variant="primary" onclick={changePw} disabled={pw_pending}>{pw_pending ? "Changing…" : "Change password"}</Button>
      </div>
    </div>

    <div class="border-t border-divider mt-4 pt-4">
      <h3 class="font-semibold">Two-factor authentication</h3>
      <div class="flex flex-col gap-3 mt-3 max-w-md">
        <Field label="Password (re-auth)"><Input type="password" bind:value={totpPw} /></Field>
        <div class="flex gap-2">
          <Button variant="secondary" onclick={enroll} disabled={enroll_pending}>{enroll_pending ? "Enrolling…" : "Enroll / re-enroll"}</Button>
          <Button variant="destructive" onclick={disable2fa} disabled={disable_pending}>{disable_pending ? "Disabling…" : "Disable 2FA"}</Button>
        </div>
        {#if enrollSecret}
          <div class="bg-appBg border border-divider rounded-xl p-3">
            <div class="text-sm text-text-muted">Secret:</div>
            <pre class="text-xs mt-1 break-all">{enrollSecret}</pre>
            {#if enrollUrl}<div class="text-xs text-text-dim mt-2 break-all">{enrollUrl}</div>{/if}
          </div>
          <Field label="TOTP code"><Input bind:value={totpCode} placeholder="123456" /></Field>
          <Button variant="primary" onclick={verify} disabled={verify_pending}>{verify_pending ? "Verifying…" : "Verify enrollment"}</Button>
        {/if}
      </div>
    </div>
  {/if}
</Card>
