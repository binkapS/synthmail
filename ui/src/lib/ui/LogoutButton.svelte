<script lang="ts">
  import { goto } from "$app/navigation";
  import { clearAuth } from "$lib/stores/auth.svelte";
  import * as Api from "$lib/api";
  import PendingButton from "./PendingButton.svelte";

  let pending = $state(false);

  async function logout() {
    if (pending) return;
    pending = true;
    try {
      await Api.logout();
    } catch {
      /* ignore */
    }
    Api.clearToken();
    clearAuth();
    pending = false;
    goto("/login");
  }
</script>

<PendingButton
  label="Logout"
  {pending}
  onclick={logout}
  class="bg-surface border border-divider hover:bg-elevated text-text-muted hover:text-text-primary px-5 py-2.5 rounded-xl transition-colors"
  busyLabel="Logging out…"
/>
