<script lang="ts">
  import { textScramble } from "../utils/animations";

  let { annotation, heading }: { annotation: string; heading: string } = $props();
</script>

<div class="mb-10">
  <span class="aim-annotation block mb-1">// {annotation}</span>
  <h2 class="s-675756f mt-3 aim-heading" use:textScramble={{ text: heading }}>
    {heading}
  </h2>
  <div class="s-c28e8ad"></div>
</div>
