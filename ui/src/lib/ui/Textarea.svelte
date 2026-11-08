<script lang="ts">
  let {
    value = $bindable(""),
    rows = 5,
    placeholder = "",
    class: className = "",
    oninput
  }: {
    value?: string;
    rows?: number;
    placeholder?: string;
    class?: string;
    oninput?: (e: Event) => void;
  } = $props();
</script>

<textarea
  rows={rows}
  placeholder={placeholder}
  bind:value
  oninput={oninput}
  class="bg-appBg border border-divider text-text-primary placeholder:text-text-dim rounded-xl px-4 py-3 base-transition focus:outline-none focus:border-focusRing focus:ring-1 focus:ring-focusRing focus:shadow-[0_0_15px_rgba(56,189,248,0.15)] {className}"
></textarea>
