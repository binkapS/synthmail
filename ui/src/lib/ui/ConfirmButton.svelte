<script lang="ts">
  let {
    label,
    confirmLabel,
    danger = false,
    pending = false,
    onConfirm
  }: {
    label: string;
    confirmLabel: string;
    danger?: boolean;
    pending?: boolean;
    onConfirm?: () => void;
  } = $props();

  let armed = $state(false);

  const baseClass = $derived(
    danger
      ? "bg-danger/10 hover:bg-danger text-danger hover:text-text-primary border border-danger/20 px-5 py-2.5 rounded-xl transition-colors"
      : "bg-brand hover:bg-indigo-500 text-text-primary font-medium px-5 py-2.5 rounded-xl shadow-lg shadow-brand/20 active:scale-[0.98] transition-all"
  );

  function handleClick() {
    if (pending) return;
    if (armed) {
      armed = false;
      onConfirm?.();
    } else {
      armed = true;
    }
  }
</script>

<span class="inline-flex items-center gap-2">
  <button
    type="button"
    disabled={pending}
    onclick={handleClick}
    class="{baseClass} disabled:opacity-70 disabled:cursor-not-allowed {armed ? 'ring-2 ring-danger/40' : ''}"
  >
    {pending ? `${label} …` : armed ? confirmLabel : label}
    {#if pending}
      <span class="w-3.5 h-3.5 rounded-full border-2 border-current border-t-transparent animate-spin inline-block"></span>
    {/if}
  </button>
  {#if armed}
    <button
      type="button"
      onclick={() => (armed = false)}
      class="bg-surface border border-divider hover:bg-elevated text-text-muted hover:text-text-primary px-5 py-2.5 rounded-xl transition-colors"
    >
      Cancel
    </button>
  {/if}
</span>
