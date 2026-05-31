<script lang="ts">
  /**
   * Single `+` mark, used as a small visual register at corners or
   * focal points. Caller positions it via `className` (which must
   * include `absolute` positioning utilities like `-top-1 -right-2`).
   *
   *   <RegistrationMark className="-top-1 -right-2" />
   *
   * Always green (`#50fa7b` resolved from theme accent). Tiny; pairs
   * naturally with `CornerBrackets`.
   */

  let { className = "" }: { className?: string } = $props();
</script>

<span
  class="absolute text-xs select-none {className}" class:s-aadcf98={true}
 
>
  +
</span>
