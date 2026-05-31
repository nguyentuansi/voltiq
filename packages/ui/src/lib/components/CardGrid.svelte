<script lang="ts" generics="T">
  /**
   * Responsive grid + reveal-on-scroll stagger animation for a list of
   * cards. Each rendered item fades-in from below as it enters the
   * viewport; `stagger` shifts each item's delay so the row appears as
   * a wave.
   *
   *   <CardGrid items={howItWorks} cols={3} stagger={150}>
   *     {#snippet item(step, i)}
   *       <Card tilt>
   *         …content using `step` and `i`…
   *       </Card>
   *     {/snippet}
   *   </CardGrid>
   *
   * Use this for *content* grids. Page-shell layouts (header, footer,
   * etc.) should use plain flex/grid utilities directly.
   */

  import type { Snippet } from "svelte";
  import { inView } from "../utils/animations";

  interface Props {
    items:    T[];
    /** Columns at the `lg:` breakpoint. Smaller breakpoints step down. */
    cols?:    1 | 2 | 3 | 4;
    /** Per-item delay in ms (item N gets `N * stagger` delay). Default 100. */
    stagger?: number;
    /** Tailwind gap class. Default `gap-2`. */
    gap?:     string;
    item:     Snippet<[T, number]>;
  }

  let { items, cols = 3, stagger = 100, gap = "gap-2", item }: Props = $props();

  const gridCols: Record<number, string> = {
    1: "grid-cols-1",
    2: "grid-cols-1 md:grid-cols-2",
    3: "grid-cols-1 md:grid-cols-2 lg:grid-cols-3",
    4: "grid-cols-1 md:grid-cols-2 lg:grid-cols-4",
  };

  let visible = $state<boolean[]>(items.map(() => false));
</script>

<div class="grid {gridCols[cols]} {gap}">
  {#each items as it, i (i)}
    <div
      use:inView={() => (visible[i] = true)} class="s-fa64f5a" style="--659: {visible[i] ? 1 : 0}; --0ef: translateY({visible[i] ? 0 : 20}px); --4c4: {i * stagger}ms;"
     
    >
      {@render item(it, i)}
    </div>
  {/each}
</div>
