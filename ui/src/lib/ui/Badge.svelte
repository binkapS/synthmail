<script lang="ts">
  import type { Snippet } from "svelte";
  let { variant = "success", class: className = "", children }: {
    variant?: "success" | "warning" | "danger";
    class?: string;
    children?: Snippet;
  } = $props();

  const variants: Record<string, string> = {
    success: "bg-success/10 text-success text-xs font-semibold px-2.5 py-1 rounded-full border border-success/20",
    warning: "bg-warning/10 text-warning text-xs font-semibold px-2.5 py-1 rounded-full border border-warning/20",
    danger: "bg-danger/10 text-danger text-xs font-semibold px-2.5 py-1 rounded-full border border-danger/20"
  };
</script>

<span class="{variants[variant]} {className}">{@render children?.()}</span>
