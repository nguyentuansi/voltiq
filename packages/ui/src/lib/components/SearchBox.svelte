<script lang="ts">
  /**
   * Icon + text-input combo used in every console toolbar. The icon
   * defaults to a magnifier; override with the `Icon` prop for "domain"
   * / "module" / "host" filters.
   *
   *   <SearchBox bind:value={search} placeholder="search..." />
   *   <SearchBox bind:value={domainFilter} Icon={Globe}
   *              placeholder="domain..." width="w-28" />
   */

  import type { Component } from "svelte";
  import { Search as DefaultIcon } from "lucide-svelte";

  interface Props {
    value:        string;
    placeholder?: string;
    Icon?:        Component;
    /** Tailwind width on the `<input>`. Default `w-36`. */
    width?:       string;
    /** Fires every input event (e.g. to reset pagination offset). */
    oninput?:     () => void;
  }

  let {
    value = $bindable(""),
    placeholder = "search...",
    Icon = DefaultIcon,
    width = "w-36",
    oninput,
  }: Props = $props();
</script>

<div class="items-center flex s-b0c54ba">
  <Icon size={12} class="ml-1.5 s-f905530 shrink-0" />
  <input
    type="text"
    bind:value
    {placeholder}
    {oninput}
    class="bg-transparent text-xs px-1.5 py-0.5 {width} focus:outline-none" class:s-697ea59={true}
   
  />
</div>
