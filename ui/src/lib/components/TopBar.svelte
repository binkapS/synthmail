<script lang="ts">
  import { page } from "$app/state";
  import * as Api from "$lib/api";
  import LogoutButton from "$lib/ui/LogoutButton.svelte";

  const path = $derived(page.url.pathname);
  const isActive = (p: string) => path === p;
  const token = $derived(Api.getToken());
</script>

<header class="sticky top-0 z-40 flex items-center gap-3 px-4 py-3 bg-surface/90 border border-divider rounded-2xl backdrop-blur">
  <a href="/" class="flex items-center gap-2 font-bold text-text-primary">
    <span class="grid place-items-center w-9 h-9 rounded-xl bg-brand text-text-primary shadow-lg shadow-brand/20">SM</span>
    <span>SynthMail</span>
  </a>
  <nav class="hidden md:flex items-center gap-1 ml-auto">
    <a href="/" class="px-3 py-2 rounded-xl transition-colors {isActive('/') ? 'bg-brand/10 text-text-primary' : 'text-text-muted hover:text-text-primary hover:bg-elevated'}">Home</a>
    <a href="/login" class="px-3 py-2 rounded-xl transition-colors {isActive('/login') ? 'bg-brand/10 text-text-primary' : 'text-text-muted hover:text-text-primary hover:bg-elevated'}">Sign in</a>
    <a href="/admin" class="px-3 py-2 rounded-xl transition-colors {isActive('/admin') ? 'bg-brand/10 text-text-primary' : 'text-text-muted hover:text-text-primary hover:bg-elevated'}">Admin</a>
    <a href="/mail" class="px-3 py-2 rounded-xl transition-colors {isActive('/mail') ? 'bg-brand/10 text-text-primary' : 'text-text-muted hover:text-text-primary hover:bg-elevated'}">Mail</a>
    <a href="/dav/" target="_blank" rel="noopener" class="px-3 py-2 rounded-xl text-text-muted hover:text-text-primary hover:bg-elevated transition-colors">DAV</a>
  </nav>
  <div class="flex items-center gap-2 md:ml-2">
    {#if token}
      <LogoutButton />
    {/if}
  </div>
</header>
