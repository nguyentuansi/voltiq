import adapter from "@sveltejs/adapter-static";
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

/** @type {import('@sveltejs/kit').Config} */
export default {
  preprocess: vitePreprocess(),
  kit: {
    // SPA: a single fallback shell, hydrated client-side and fed by voltiq's
    // local JSON/SSE API. The built `build/` dir is embedded into the Rust binary.
    adapter: adapter({ fallback: "index.html", pages: "build", assets: "build" }),
    alias: {
      $api: "src/lib/api",
    },
  },
};
