<script lang="ts">
  let {
    label,
    pending = false,
    class: className = "bg-brand hover:bg-indigo-500 text-text-primary font-medium px-5 py-2.5 rounded-xl shadow-lg shadow-brand/20 active:scale-[0.98] transition-all",
    btnType = "button",
    busyLabel = "",
    onclick
  }: {
    label: string;
    pending?: boolean;
    class?: string;
    btnType?: string;
    busyLabel?: string;
    onclick?: () => void;
  } = $props();
</script>

<button
  type={btnType as "button" | "submit" | "reset"}
  disabled={pending}
  onclick={() => {
    if (btnType !== "submit" && !pending) onclick?.();
  }}
  class="{className} disabled:opacity-70 disabled:cursor-not-allowed"
>
  {pending ? (busyLabel || `${label} …`) : label}
  {#if pending}
    <span class="w-3.5 h-3.5 rounded-full border-2 border-current border-t-transparent animate-spin inline-block"></span>
  {/if}
</button>
