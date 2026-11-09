<script lang="ts">
  import { goto } from "$app/navigation";
  import * as Api from "$lib/api";
  import AuthGuard from "$lib/components/AuthGuard.svelte";
  import { Card, Spinner, Button } from "$lib/ui";
  import OverviewPanel from "$lib/components/admin/OverviewPanel.svelte";
  import ServerPanel from "$lib/components/admin/ServerPanel.svelte";
  import DomainsPanel from "$lib/components/admin/DomainsPanel.svelte";
  import UsersPanel from "$lib/components/admin/UsersPanel.svelte";
  import AliasesPanel from "$lib/components/admin/AliasesPanel.svelte";
  import QuotaPanel from "$lib/components/admin/QuotaPanel.svelte";
  import QueuesPanel from "$lib/components/admin/QueuesPanel.svelte";
  import DkimPanel from "$lib/components/admin/DkimPanel.svelte";
  import FiltersPanel from "$lib/components/admin/FiltersPanel.svelte";
  import VacationPanel from "$lib/components/admin/VacationPanel.svelte";
  import DmarcPanel from "$lib/components/admin/DmarcPanel.svelte";
  import AuditPanel from "$lib/components/admin/AuditPanel.svelte";
  import BansPanel from "$lib/components/admin/BansPanel.svelte";
  import TokensPanel from "$lib/components/admin/TokensPanel.svelte";
  import ConfigPanel from "$lib/components/admin/ConfigPanel.svelte";
  import AccountPanel from "$lib/components/admin/AccountPanel.svelte";
  import { setAuthToken } from "$lib/stores/auth.svelte";

  const tabs: [string, string][] = [
    ["overview", "Overview"],
    ["server", "Server"],
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
  let sidebarOpen = $state(false);

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
    <button
      type="button"
      class="lg:hidden mb-3 bg-surface/70 border border-divider hover:bg-elevated text-text-muted hover:text-text-primary px-4 py-2.5 rounded-2xl transition-colors flex items-center gap-2"
      onclick={() => (sidebarOpen = !sidebarOpen)}
      aria-expanded={sidebarOpen}
    >
      <svg class="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M4 6h16M4 12h16M4 18h16" /></svg>
      {sidebarOpen ? "Hide menu" : "Admin menu"}
    </button>
    <div class="grid grid-cols-1 lg:grid-cols-[280px_1fr] gap-5">
      <aside class="{sidebarOpen ? 'flex' : 'hidden'} lg:flex bg-surface/70 backdrop-blur-xl border border-divider rounded-3xl p-4 flex-col gap-2 h-fit lg:sticky lg:top-4 shadow-2xl shadow-black/20">
        <div class="flex items-center gap-3 px-2 py-2">
          <div class="grid place-items-center w-10 h-10 rounded-2xl bg-gradient-to-br from-brand to-info shadow-lg shadow-brand/20 text-sm font-bold">A</div>
          <div>
            <div class="font-semibold">Admin console</div>
            <div class="text-xs text-text-muted">{meEmail || "Signed in"}</div>
          </div>
        </div>
        <nav class="flex flex-col gap-1 mt-2">
          {#each tabs as [key, label]}
            <button
              type="button"
              onclick={() => { active = key; sidebarOpen = false; }}
              class="text-left w-full px-3 py-2.5 rounded-xl transition-all {active === key ? 'bg-gradient-to-r from-brand/20 to-info/10 text-text-primary shadow-inner border border-brand/20' : 'text-text-muted hover:text-text-primary hover:bg-elevated/70'}"
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
        {#if active === "overview"}<OverviewPanel />
        {:else if active === "server"}<ServerPanel />
        {:else if active === "domains"}<DomainsPanel />
        {:else if active === "users"}<UsersPanel />
        {:else if active === "aliases"}<AliasesPanel />
        {:else if active === "quota"}<QuotaPanel />
        {:else if active === "queues"}<QueuesPanel />
        {:else if active === "dkim"}<DkimPanel />
        {:else if active === "filters"}<FiltersPanel />
        {:else if active === "vacation"}<VacationPanel />
        {:else if active === "dmarc"}<DmarcPanel />
        {:else if active === "audit"}<AuditPanel />
        {:else if active === "bans"}<BansPanel />
        {:else if active === "tokens"}<TokensPanel />
        {:else if active === "config"}<ConfigPanel />
        {:else if active === "account"}<AccountPanel />
        {/if}
      </section>
    </div>
  {/if}
</AuthGuard>
