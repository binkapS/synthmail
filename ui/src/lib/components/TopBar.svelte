<script lang="ts">
  import { page } from "$app/state";
  import * as Api from "$lib/api";
  import LogoutButton from "$lib/ui/LogoutButton.svelte";

  const path = $derived(page.url.pathname);
  const isActive = (p: string) => path === p;
  const token = $derived(Api.getToken());
</script>

<header class="sticky top-3 z-40 flex items-center gap-3 px-4 py-2.5 rounded-2xl border border-divider/80 bg-surface/70 backdrop-blur-xl shadow-xl shadow-black/20">
  <a href="/" class="flex items-center gap-2.5 font-bold text-text-primary">
    <span class="grid place-items-center w-10 h-10 rounded-xl bg-gradient-to-br from-brand to-info text-text-primary shadow-lg shadow-brand/30">
      <svg class="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="m22 2 7 7m-10 3-2 2-3-3 2-2m-4-1 9-9m-1 8-8 8H3v-5l8-8"/></svg>
    </span>
    <span class="text-lg tracking-tight">SynthMail</span>
  </a>

  <nav class="hidden md:flex items-center gap-1 ml-auto bg-elevated/60 rounded-xl p-1 border border-divider/60">
    <a href="/" class="px-4 py-1.5 rounded-lg text-sm font-medium transition-all {isActive('/') ? 'bg-brand text-text-primary shadow shadow-brand/30' : 'text-text-muted hover:text-text-primary'}">Home</a>
    <a href="/admin" class="px-4 py-1.5 rounded-lg text-sm font-medium transition-all {isActive('/admin') ? 'bg-brand text-text-primary shadow shadow-brand/30' : 'text-text-muted hover:text-text-primary'}">Admin</a>
    <a href="/mail" class="px-4 py-1.5 rounded-lg text-sm font-medium transition-all {isActive('/mail') ? 'bg-brand text-text-primary shadow shadow-brand/30' : 'text-text-muted hover:text-text-primary'}">Mail</a>
    {#if !token}
      <a href="/login" class="px-4 py-1.5 rounded-lg text-sm font-medium transition-all {isActive('/login') ? 'bg-brand text-text-primary shadow shadow-brand/30' : 'text-text-muted hover:text-text-primary'}">Sign in</a>
    {/if}
    <a href="/dav/" target="_blank" rel="noopener" class="px-4 py-1.5 rounded-lg text-sm font-medium text-text-muted hover:text-text-primary transition-all">DAV</a>
  </nav>

  <div class="flex items-center gap-2 md:ml-3">
    {#if token}
      <LogoutButton />
    {/if}
  </div>
</header>
