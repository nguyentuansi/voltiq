<script lang="ts">
  /**
   * Coloured CTA anchor that lights up on hover. Use for "Access
   * Console", "Read sample report", "Sign up", etc.
   *
   *   <AccentLink href="/sign-up" accent="#50fa7b">
   *     Sign up <ArrowRight size={14} />
   *   </AccentLink>
   *
   *   <!-- Filled variant (CTA fills with accent on hover) -->
   *   <AccentLink href="/pro" accent={tier.color} variant="fill">
   *     Choose plan
   *   </AccentLink>
   *
   * `variant="border"` (default) — transparent background, accent
   * border, accent text. Hover: stronger border + soft glow.
   * `variant="fill"` — same idle look; hover fills the background
   * with the accent colour and inverts text to bg colour.
   */

  import type { Snippet } from "svelte";

  interface Props {
    href:     string;
    accent?:  string;
    variant?: "border" | "fill";
    target?:  string;
    rel?:     string;
    title?:   string;
    /** Tailwind padding. Default `py-3 px-5`. */
    padding?: string;
    /** Tailwind extras (e.g. `inline-flex gap-2 items-center`). */
    class?:   string;
    children: Snippet;
  }

  let {
    href,
    accent = "var(--aim-accent)",
    variant = "border",
    target,
    rel,
    title,
    padding = "py-3 px-5",
    class: extra = "",
    children,
  }: Props = $props();

  // Pre-compute hover colour mixes so the handlers can reach them.
  const idleBorder = $derived(variant === "fill" ? `${accent}55` : `${accent}40`);
  const hoverBg    = $derived(`color-mix(in srgb, ${accent} 8%, transparent)`);
  const hoverGlow  = $derived(`0 0 14px color-mix(in srgb, ${accent} 25%, transparent)`);

  function onEnter(e: MouseEvent) {
    const el = e.currentTarget as HTMLElement;
    if (variant === "fill") {
      el.style.backgroundColor = accent;
      el.style.color           = "#1c1b19";
    } else {
      el.style.backgroundColor = hoverBg;
      el.style.boxShadow       = hoverGlow;
    }
    el.style.borderColor = accent;
  }
  function onLeave(e: MouseEvent) {
    const el = e.currentTarget as HTMLElement;
    el.style.backgroundColor = "transparent";
    el.style.boxShadow       = "none";
    el.style.borderColor     = idleBorder;
    el.style.color           = accent;
  }
</script>

<a
  {href} {target} {rel} {title}
  class="inline-flex items-center gap-2 text-xs font-bold uppercase {padding} {extra}"
 
  onmouseenter={onEnter}
  onmouseleave={onLeave} class:s-eb50852={true} style="--904: {accent}; --f60: 1px solid {idleBorder};"
>
  {@render children()}
</a>
