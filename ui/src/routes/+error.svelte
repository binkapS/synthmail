<script lang="ts">
  import { page } from "$app/state";
</script>

<div class="grid place-items-center min-h-screen bg-appBg text-text-primary p-6">
  <div class="bg-surface border border-divider rounded-2xl p-6 shadow-xl max-w-md w-full">
    <h1 class="text-xl font-semibold">{page.status}</h1>
    <p class="text-text-muted mt-2">{page.error?.message ?? "Page not found"}</p>
    <a href="/" class="bg-brand hover:bg-indigo-500 text-text-primary font-medium px-5 py-2.5 rounded-xl shadow-lg shadow-brand/20 active:scale-[0.98] transition-all inline-block mt-4">Home</a>
  </div>
</div>
