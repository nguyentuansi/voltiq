<script lang="ts">
  import "../app.css";
  import { QueryClient, QueryClientProvider } from "@tanstack/svelte-query";
  import { setToastContext, ToastStack } from "@landing-v/ui";

  // One QueryClient per tab. Failures against the local API are bugs, not transient
  // network errors, so don't retry.
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
  });

  setToastContext();

  let { children } = $props();
</script>

<QueryClientProvider client={queryClient}>
  <div class="min-h-screen flex flex-col">
    <header
      class="px-4 py-2 text-sm flex items-center gap-2"
      style="border-bottom:1px solid var(--aim-border)"
    >
      <span style="color:var(--aim-accent)">&gt;</span>
      <span class="font-bold tracking-wide">VOLTIQ</span>
      <span style="color:var(--aim-text-muted)">— performance &amp; security report</span>
    </header>
    <main class="flex-1 py-3 md:px-4 px-2">
      {@render children()}
    </main>
  </div>
  <ToastStack />
</QueryClientProvider>
