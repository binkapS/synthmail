<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    variant = "primary",
    type = "button",
    disabled = false,
    class: className = "",
    onclick,
    children
  }: {
    variant?: "primary" | "secondary" | "destructive";
    type?: "button" | "submit" | "reset";
    disabled?: boolean;
    class?: string;
    onclick?: (e: MouseEvent) => void;
    children?: Snippet;
  } = $props();

  const variants: Record<string, string> = {
    primary:
      "bg-brand hover:bg-indigo-500 text-text-primary font-medium px-5 py-2.5 rounded-xl shadow-lg shadow-brand/20 active:scale-[0.98] transition-all",
    secondary:
      "bg-surface border border-divider hover:bg-elevated text-text-muted hover:text-text-primary px-5 py-2.5 rounded-xl transition-colors",
    destructive:
      "bg-danger/10 hover:bg-danger text-danger hover:text-text-primary border border-danger/20 px-5 py-2.5 rounded-xl transition-colors"
  };
</script>

<button
  type={type}
  disabled={disabled}
  onclick={onclick}
  class="{variants[variant]} disabled:opacity-50 disabled:cursor-not-allowed {className}"
>
  {@render children?.()}
</button>
