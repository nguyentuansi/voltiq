<script lang="ts">
  /**
   * Large-number stat card with accent-coloured value and label. Sized
   * for 3-up grids; place inside a `<div class="grid gap-3 grid-cols-3">`.
   *
   *   <Stat value="2,400+" label="findings shipped" accent="#50fa7b" />
   *
   *   <!-- With on-load counter scramble -->
   *   <Stat value="2,400+" label="findings shipped" accent="#50fa7b" scramble />
   *
   * Always renders inside a bordered translucent panel with
   * CornerBrackets and a hover-glow tuned to `accent`.
   */

  import CornerBrackets from "./CornerBrackets.svelte";
  import { scrambleCounter } from "../utils/animations";

  interface Props {
    value:    string;
    label:    string;
    /** Hex or CSS colour for the large number + hover glow. */
    accent?:  string;
    /** Use the digit-rolling reveal action. */
    scramble?: boolean;
  }

  let { value, label, accent = "#fce8c3", scramble = false }: Props = $props();

  function onEnter(e: MouseEvent) {
    const el = e.currentTarget as HTMLElement;
    el.style.borderColor = `${accent}55`;
    el.style.boxShadow   = `0 0 16px ${accent}1a`;
  }
  function onLeave(e: MouseEvent) {
    const el = e.currentTarget as HTMLElement;
    el.style.borderColor = `${accent}22`;
    el.style.boxShadow   = "none";
  }
</script>

<div
  class="text-center relative s-222280c p-5"
 
  onmouseenter={onEnter}
  onmouseleave={onLeave}
  role="group" style="--f60: 1px solid {accent}22;"
>
  <CornerBrackets />
  <div
    class="s-f75a665 aim-heading" style="--904: {accent}; --32b: 0 0 18px {accent}33;"
   
  >
    {#if scramble}
      <span use:scrambleCounter={{ value }}>0</span>
    {:else}
      {value}
    {/if}
  </div>
  <div class="s-f905530 text-xs uppercase tracking-wider mt-2">
    {label}
  </div>
</div>
