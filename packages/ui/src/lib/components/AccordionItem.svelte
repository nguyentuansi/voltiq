<script lang="ts">
  /**
   * Single accordion row — bordered, click-to-expand, `+`/`−` icon.
   *
   * Solo (each row owns its own state):
   *   <AccordionItem question="…" bind:open>
   *     {#snippet body()}…{/snippet}
   *   </AccordionItem>
   *
   * Exclusive group (only one open at a time):
   *   {#each items as it, i}
   *     <AccordionItem
   *       question={it.q}
   *       open={selected === i}
   *       onToggle={() => (selected = selected === i ? null : i)}
   *     >
   *       {#snippet body()}…{/snippet}
   *     </AccordionItem>
   *   {/each}
   *
   * `onToggle` takes precedence; if absent, the component flips its
   * own bound `open`.
   */

  import { slide } from "svelte/transition";
  import { Plus, Minus } from "lucide-svelte";
  import type { Snippet } from "svelte";

  interface Props {
    question:  string;
    open?:     boolean;
    /** External toggle handler (overrides internal flip). */
    onToggle?: () => void;
    accent?:   string;
    body:      Snippet;
  }

  let {
    question,
    open = $bindable(false),
    onToggle,
    accent = "var(--aim-accent)",
    body,
  }: Props = $props();

  function toggle() {
    if (onToggle) onToggle();
    else open = !open;
  }
</script>

<div class="s-304757d" style="--f60: 1px solid {open ? accent : 'rgba(252,232,195,0.08)'}; --691: {open ? `0 0 14px color-mix(in srgb, ${accent} 15%, transparent)` : 'none'};"
 
>
  <button
    type="button"
    onclick={toggle}
    class="items-center px-5 flex text-left w-full py-4 justify-between gap-4"
  >
    <span class="s-2cdcebf">{question}</span>
    <span class="s-f2727fe" style="--904: {open ? accent : 'var(--aim-text-muted)'};">
      {#if open}<Minus size={16} />{:else}<Plus size={16} />{/if}
    </span>
  </button>

  {#if open}
    <div transition:slide={{ duration: 250 }} class="s-c5f6c40">
      <div class="px-5 aim-body py-4">
        {@render body()}
      </div>
    </div>
  {/if}
</div>
