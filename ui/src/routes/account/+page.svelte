<script lang="ts">
  import AuthGuard from "$lib/components/AuthGuard.svelte";
  import AccountPanel from "$lib/components/admin/AccountPanel.svelte";
</script>

<AuthGuard>
  <div class="max-w-3xl mx-auto space-y-4">
    <h1 class="text-2xl font-bold">Account</h1>
    <AccountPanel />
  </div>
</AuthGuard>
