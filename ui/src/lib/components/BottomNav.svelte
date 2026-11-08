<script lang="ts">
  import { page } from "$app/state";
  import * as Api from "$lib/api";
  const path = $derived(page.url.pathname);
  const active = (p: string) => path === p;
  const authed = $derived(!!Api.getToken());
  const items = [
    ["/", "Home", "M3 10l9-7 9 7v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z M9 22V12h6v10"],
    ["/admin", "Admin", "M12 12a3 3 0 1 0 0-6 3 3 0 0 0 0 6"],
    ["/mail", "Mail", "M2 4h20v16H2z m0 3 10 7 10-7"],
    ["/dav/", "DAV", "M15 3h6v6 M10 14 21 3 M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"]
  ];
</script>

<nav class="md:hidden fixed bottom-3 left-3 right-3 z-40 grid gap-1 p-2 rounded-2xl border border-divider bg-surface/85 backdrop-blur-xl shadow-2xl shadow-black/30" style="grid-template-columns: repeat({authed ? 4 : 5}, minmax(0, 1fr))">
  {#if !authed}
    <a href="/login" class="flex flex-col items-center justify-center gap-0.5 min-h-14 rounded-xl text-[11px] font-semibold transition-all {active('/login') ? 'bg-brand text-text-primary shadow shadow-brand/30' : 'text-text-muted hover:text-text-primary hover:bg-elevated/70'}">
      <svg class="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4 m10 17 5-5-5-5 M15 12H3" /></svg>
      Sign in
    </a>
  {/if}
  {#each items as [href, label, d]}
    <a href={href} class="flex flex-col items-center justify-center gap-0.5 min-h-14 rounded-xl text-[11px] font-semibold transition-all {active(href) ? 'bg-brand text-text-primary shadow shadow-brand/30' : 'text-text-muted hover:text-text-primary hover:bg-elevated/70'}">
      <svg class="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d={d} /></svg>
      {label}
    </a>
  {/each}
</nav>
