import { sveltekit } from "@sveltejs/kit/vite";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [tailwindcss(), sveltekit()],
  // @landing-v/ui is consumed as Svelte source from the workspace; keep it bundled.
  ssr: { noExternal: ["@landing-v/ui"] },
  server: { port: 7879 },
});
