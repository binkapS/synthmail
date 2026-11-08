<script lang="ts">
  let {
    value = $bindable(""),
    type = "text",
    placeholder = "",
    class: className = "",
    oninput
  }: {
    value?: string;
    type?: string;
    placeholder?: string;
    class?: string;
    oninput?: (e: Event) => void;
  } = $props();
</script>

<input
  type={type}
  placeholder={placeholder}
  bind:value
  oninput={oninput}
  class="bg-appBg border border-divider text-text-primary placeholder:text-text-dim rounded-xl px-4 py-3 base-transition focus:outline-none focus:border-focusRing focus:ring-1 focus:ring-focusRing focus:shadow-[0_0_15px_rgba(56,189,248,0.15)] {className}"
/>
