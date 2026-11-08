<script lang="ts">
  import { page } from "$app/state";
  import * as Api from "$lib/api";
  import { auth } from "$lib/stores/auth.svelte";

  const path = $derived(page.url.pathname);
  const active = (p: string) => path === p;
  const authed = $derived(!!auth.token);
  let isAdmin = $state(false);

  $effect(() => {
    if (!authed) { isAdmin = false; return; }
    (async () => {
      try {
        const me = await Api.me();
        isAdmin = me.kind === "admin" || me.kind === "superadmin";
      } catch {
        isAdmin = false;
      }
    })();
  });

  const icons: Record<string, string> = {
    "/login": "M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4 m10 17 5-5-5-5 M15 12H3",
    "/": "M3 10l9-7 9 7v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z M9 22V12h6v10",
    "/mail": "M2 4h20v16H2z m0 3 10 7 10-7",
    "/account": "M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8 M4 20c0-4 4-7 8-7s8 3 8 7",
    "/admin": "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6 M19 12h.01 M5 12h.01",
    "/dav/": "M15 3h6v6 M10 14 21 3 M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"
  };

  const navItems = $derived.by(() => {
    const out: [string, string][] = [];
    if (!authed) out.push(["/login", "Sign in"]);
    if (isAdmin) out.push(["/", "Home"]);
    if (authed) out.push(["/mail", "Mail"]);
    if (authed) out.push(["/account", "Account"]);
    if (isAdmin) out.push(["/admin", "Admin"]);
    out.push(["/dav/", "DAV"]);
    return out;
  });
</script>

<nav
  class="md:hidden fixed bottom-3 left-3 right-3 z-40 grid gap-1 p-2 rounded-2xl border border-divider bg-surface/85 backdrop-blur-xl shadow-2xl shadow-black/30"
  style="grid-template-columns: repeat({navItems.length}, minmax(0, 1fr))"
>
  {#each navItems as [href, label]}
    <a
      href={href}
      class="flex flex-col items-center justify-center gap-0.5 min-h-14 rounded-xl text-[11px] font-semibold transition-all {active(href) ? 'bg-brand text-text-primary shadow shadow-brand/30' : 'text-text-muted hover:text-text-primary hover:bg-elevated/70'}"
    >
      <svg class="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d={icons[href]} /></svg>
      {label}
    </a>
  {/each}
</nav>
