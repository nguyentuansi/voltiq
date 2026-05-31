<script lang="ts">
  /**
   * Toggleable bracket-row of filter pills sharing a single value. Each
   * option becomes a chip; the matching chip is highlighted with the
   * `tone` colour.
   *
   *   <FilterChips
   *     bind:value={statusFilter}
   *     tone="accent"
   *     options={[
   *       { v: "",          label: "all" },
   *       { v: "running",   label: "running" },
   *       { v: "completed", label: "completed" },
   *     ]}
   *   />
   *
   * Used for status/severity/method filter rows. For richer dropdowns
   * (e.g. mode selectors) prefer a plain `<select>` element.
   */

  type Tone = "accent" | "secondary" | "tertiary";

  interface Option<V> {
    v:     V;
    label: string;
  }

  interface Props<V extends string = string> {
    value:    V;
    options:  Option<V>[];
    tone?:    Tone;
    /** Fires after the user picks an option. */
    onchange?: (v: V) => void;
  }

  let {
    value = $bindable(),
    options,
    tone = "accent",
    onchange,
  }: Props = $props();

  const COLOR: Record<Tone, string> = {
    accent:    "var(--aim-accent)",
    secondary: "var(--aim-secondary)",
    tertiary:  "var(--aim-tertiary)",
  };
  const c = $derived(COLOR[tone]);
</script>

<div class="s-1761a3a flex">
  {#each options as opt (opt.v)}
    <button
      onclick={() => {
        value = opt.v;
        onchange?.(opt.v);
      }}
      class="text-xs uppercase py-0.5 s-e5c7897 px-2" style="--0c0: {value === opt.v ? c : 'var(--aim-text-muted)'}; --a32: {value === opt.v ? `color-mix(in srgb, ${c} 10%, transparent)` : 'transparent'};"
     
    >{opt.label}</button>
  {/each}
</div>
