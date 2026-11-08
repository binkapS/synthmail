<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { getToken } from "$lib/api";
  import type { Snippet } from "svelte";
  let { children }: { children?: Snippet } = $props();

  onMount(() => {
    if (!getToken()) goto("/login");
  });
</script>

{@render children?.()}
