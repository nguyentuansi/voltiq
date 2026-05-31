<script lang="ts">
  /**
   * Terminal-style `[label]` button. The brackets are literal in the
   * children, so callers control them:
   *
   *   <BracketButton tone="error" onclick={stop}>[stop]</BracketButton>
   *   <BracketButton tone="accent" href="/scan/new">[+ new scan]</BracketButton>
   *
   * `tone` selects from the theme palette. `variant="boxed"` adds a
   * 1px border around the label (used by upload / new-ingest CTAs).
   * Pass `href` to render an `<a>` instead of `<button>`.
   */

  import type { Snippet } from "svelte";

  type Tone = "accent" | "secondary" | "tertiary" | "success" | "error" | "muted" | "text";

  interface Props {
    tone?:    Tone;
    variant?: "inline" | "boxed";
    href?:    string;
    onclick?: (e: MouseEvent) => void;
    title?:   string;
    disabled?: boolean;
    children: Snippet;
  }

  let {
    tone = "muted",
    variant = "inline",
    href,
    onclick,
    title,
    disabled = false,
    children,
  }: Props = $props();

  const COLOR: Record<Tone, string> = {
    accent:    "var(--aim-accent)",
    secondary: "var(--aim-secondary)",
    tertiary:  "var(--aim-tertiary)",
    success:   "var(--aim-success)",
    error:     "var(--aim-error)",
    muted:     "var(--aim-text-muted)",
    text:      "var(--aim-text)",
  };

  const baseStyle = $derived(
    variant === "boxed"
      ? `color: ${COLOR[tone]}; border: 1px solid color-mix(in srgb, ${COLOR[tone]} 50%, transparent);`
      : `color: ${COLOR[tone]};`,
  );
  const padding = variant === "boxed" ? "px-2 py-0.5" : "";
</script>

{#if href}
  <a {href} {title} class="{padding} text-xs font-bold uppercase" style={baseStyle}>
    {@render children()}
  </a>
{:else}
  <button {onclick} {title} {disabled} class="{padding} text-xs" style={baseStyle}>
    {@render children()}
  </button>
{/if}
