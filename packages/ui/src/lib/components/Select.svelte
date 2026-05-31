<script lang="ts">
  /**
   * Console-styled `<select>` dropdown. Matches the bordered text-xs
   * look used by mode / source / type filters.
   *
   *   <Select bind:value={modeFilter}>
   *     <option value="">mode:all</option>
   *     <option>autopilot</option>
   *     <option>swarm</option>
   *   </Select>
   *
   * Pass options as children for the most flexibility (groups, disabled
   * entries, custom labels).
   */

  import type { Snippet } from "svelte";

  interface Props {
    value:     string;
    onchange?: (e: Event) => void;
    children:  Snippet;
  }

  let { value = $bindable(""), onchange, children }: Props = $props();
</script>

<select
  bind:value
  {onchange}
  class="s-71a3e16 text-xs focus:outline-none px-1.5 py-0.5"
 
>
  {@render children()}
</select>
