<script lang="ts">
  /**
   * Landing-page section wrapper. Standardises padding, scroll anchor,
   * max-width inner column, and the `aim-section` class which gives the
   * scroll-margin tween + positioning.
   *
   *   <Section id="how-it-works">
   *     <SectionHeading annotation="…" heading="…" />
   *     …
   *   </Section>
   *
   * Pass `wide` to widen the inner column (used by Showcase). Pass
   * `extraClass` for one-off positioning overrides (e.g. `relative`).
   */

  import type { Snippet } from "svelte";

  interface Props {
    id?:         string;
    /** Tailwind max-width on the inner column. Default `max-w-6xl`. */
    maxWidth?:   string;
    /** Extra utility classes on the `<section>`. */
    class?:      string;
    children:    Snippet;
  }

  let { id, maxWidth = "max-w-6xl", class: extra = "", children }: Props = $props();
</script>

<section {id} class="py-8 px-6 md:py-12 aim-section {extra}">
  <div class="mx-auto {maxWidth}">
    {@render children()}
  </div>
</section>
