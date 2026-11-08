<script lang="ts">
  import { goto } from "$app/navigation";
  import * as Api from "$lib/api";
  import AuthGuard from "$lib/components/AuthGuard.svelte";
  import { Card, Spinner, Button } from "$lib/ui";
  import { setAuthToken } from "$lib/stores/auth.svelte";

  const tabs: [string, string][] = [
    ["overview", "Overview"],
    ["domains", "Domains"],
    ["users", "Users"],
    ["aliases", "Aliases"],
    ["quota", "Quota"],
    ["queues", "Queues"],
    ["dkim", "DKIM"],
    ["filters", "Filters"],
    ["vacation", "Vacation"],
    ["dmarc", "DMARC"],
    ["audit", "Audit"],
    ["bans", "Bans"],
    ["tokens", "Tokens"],
    ["config", "Config"],
    ["account", "Account"]
  ];

  let admin = $state<boolean | null>(null);
  let active = $state("overview");
  let meEmail = $state("");

  $effect(() => {
    if (!Api.getToken()) {
      goto("/login");
      return;
    }
    (async () => {
      try {
        const me = await Api.me();
        meEmail = me.email ?? "";
        admin = me.kind === "admin" || me.kind === "superadmin";
      } catch {
        admin = false;
      }
    })();
  });
</script>

<AuthGuard>
  {#if admin === null}
    <div class="p-6"><Spinner /></div>
  {:else if admin === false}
    <Card>
      <h2 class="text-xl font-semibold">Admin access required</h2>
      <p class="text-text-muted">Your account does not have the admin role. Sign in with an admin account to use these tools.</p>
      <div class="flex gap-3 mt-4">
        <a href="/login" class="bg-brand hover:bg-indigo-500 text-text-primary font-medium px-5 py-2.5 rounded-xl shadow-lg shadow-brand/20 active:scale-[0.98] transition-all">Sign in</a>
        <a href="/mail" class="bg-surface border border-divider hover:bg-elevated text-text-muted hover:text-text-primary px-5 py-2.5 rounded-xl transition-colors">Open webmail</a>
      </div>
    </Card>
  {:else}
    <div class="grid grid-cols-1 md:grid-cols-[256px_1fr] gap-4">
      <aside class="bg-surface border border-divider rounded-2xl p-4 flex flex-col gap-2 h-fit md:sticky md:top-4">
        <h3 class="text-lg font-semibold">Admin</h3>
        <nav class="flex flex-col gap-1">
          {#each tabs as [key, label]}
            <button
              type="button"
              onclick={() => (active = key)}
              class="text-left w-full px-3 py-2 rounded-xl transition-colors {active === key ? 'bg-brand/10 border-l-4 border-l-brand text-text-primary' : 'text-text-muted hover:text-text-primary hover:bg-elevated'}"
            >
              {label}
            </button>
          {/each}
        </nav>
        <div class="mt-auto pt-4 border-t border-divider">
          <Button variant="secondary" class="w-full" onclick={() => { Api.clearToken(); setAuthToken(null); goto('/login'); }}>Logout</Button>
        </div>
      </aside>
      <section class="min-w-0 flex flex-col gap-4">
        <Card>
          <h2 class="text-xl font-semibold">{tabs.find((t) => t[0] === active)?.[1] ?? "Overview"}</h2>
          <p class="text-text-muted text-sm">Section scaffold. Implementation follows in Phase 3.</p>
          {#if meEmail}
            <p class="text-text-dim text-xs mt-2">Signed in as {meEmail}</p>
          {/if}
        </Card>
      </section>
    </div>
  {/if}
</AuthGuard>
