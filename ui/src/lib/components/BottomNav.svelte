<script lang="ts">
  import { page } from "$app/state";
  const path = $derived(page.url.pathname);
  const active = (p: string) => path === p;
</script>

<nav class="md:hidden fixed bottom-0 left-0 right-0 z-40 grid grid-cols-5 gap-1 p-2 bg-surface/95 border-t border-divider backdrop-blur">
  <a href="/" class="flex flex-col items-center justify-center gap-0.5 min-h-14 rounded-xl text-xs font-semibold transition-colors {active('/') ? 'bg-brand/10 text-text-primary' : 'text-text-muted'}">
    <svg class="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m3 10 9-7 9 7v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><path d="M9 22V12h6v10"/></svg>
    Home
  </a>
  <a href="/login" class="flex flex-col items-center justify-center gap-0.5 min-h-14 rounded-xl text-xs font-semibold transition-colors {active('/login') ? 'bg-brand/10 text-text-primary' : 'text-text-muted'}">
    <svg class="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4"/><path d="m10 17 5-5-5-5"/><path d="M15 12H3"/></svg>
    Sign in
  </a>
  <a href="/admin" class="flex flex-col items-center justify-center gap-0.5 min-h-14 rounded-xl text-xs font-semibold transition-colors {active('/admin') ? 'bg-brand/10 text-text-primary' : 'text-text-muted'}">
    <svg class="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
    Admin
  </a>
  <a href="/mail" class="flex flex-col items-center justify-center gap-0.5 min-h-14 rounded-xl text-xs font-semibold transition-colors {active('/mail') ? 'bg-brand/10 text-text-primary' : 'text-text-muted'}">
    <svg class="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="20" height="16" x="2" y="4" rx="2"/><path d="m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7"/></svg>
    Mail
  </a>
  <a href="/dav/" class="flex flex-col items-center justify-center gap-0.5 min-h-14 rounded-xl text-xs font-semibold text-text-muted">
    <svg class="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15 3h6v6"/><path d="M10 14 21 3"/><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/></svg>
    DAV
  </a>
</nav>
