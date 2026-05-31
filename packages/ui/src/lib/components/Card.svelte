<script lang="ts">
  /**
   * Bordered card with terminal-aesthetic styling. Optionally adds a
   * 3D tilt-on-hover behaviour for capability/feature grids.
   *
   *   <Card tilt>
   *     <CornerBrackets />
   *     <h3 class="aim-heading">{title}</h3>
   *     <p class="aim-body">{body}</p>
   *   </Card>
   *
   * The default bordered + translucent look matches what Capabilities and
   * HowItWorks already use; pass `class` to override the padding or add
   * extra utilities, or `style` for one-off colour overrides.
   */

  import type { Snippet } from "svelte";

  interface Props {
    /** Adds 3D tilt + hover-glow on mouseenter/move/leave. */
    tilt?:     boolean;
    /** Tailwind padding string. Default `p-6`. */
    padding?:  string;
    /** Extra utility classes to append (e.g. `h-full`). */
    class?:    string;
    /** Pass-through inline style for one-off accents. */
    style?:    string;
    children:  Snippet;
  }

  let {
    tilt     = false,
    padding  = "p-6",
    class:   extra = "",
    style    = "",
    children,
  }: Props = $props();

  function onTiltMove(e: MouseEvent) {
    const el = e.currentTarget as HTMLElement;
    const r  = el.getBoundingClientRect();
    const x  = (e.clientX - r.left) / r.width  - 0.5;
    const y  = (e.clientY - r.top)  / r.height - 0.5;
    el.style.transform = `perspective(600px) rotateY(${x * 8}deg) rotateX(${-y * 8}deg)`;
  }
  function onTiltEnter(e: MouseEvent) {
    const el = e.currentTarget as HTMLElement;
    el.style.backgroundColor = "rgba(252,232,195,0.03)";
    el.style.borderColor     = "rgba(80,250,123,0.4)";
  }
  function onTiltLeave(e: MouseEvent) {
    const el = e.currentTarget as HTMLElement;
    el.style.transform       = "perspective(600px) rotateY(0deg) rotateX(0deg)";
    el.style.backgroundColor = "transparent";
    el.style.borderColor     = "rgba(252,232,195,0.06)";
  }
</script>

<div
  class="{padding} relative {extra}"
  style="border: 1px solid rgba(252,232,195,0.06); transition: transform 0.25s ease, background-color 0.25s ease, border-color 0.25s ease; {style}"
  role="article"
  onmousemove={tilt ? onTiltMove  : null}
  onmouseenter={tilt ? onTiltEnter : null}
  onmouseleave={tilt ? onTiltLeave : null}
>
  {@render children()}
</div>
