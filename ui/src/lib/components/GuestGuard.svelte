<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import * as Api from "$lib/api";
  import type { Snippet } from "svelte";
  let { children }: { children?: Snippet } = $props();

  onMount(async () => {
    if (Api.getToken()) {
      try {
        const me = await Api.me();
        if (me.kind === "admin" || me.kind === "superadmin") goto("/admin");
        else goto("/mail");
      } catch {
        /* stay on login */
      }
    }
  });
</script>

{@render children?.()}
