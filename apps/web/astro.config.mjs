// Static download site (spec §12), deployed to Cloudflare Pages.
import react from "@astrojs/react";
import sitemap from "@astrojs/sitemap";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "astro/config";

export default defineConfig({
  // The public address; set SITE_URL once a custom domain exists.
  site: process.env.SITE_URL ?? "https://jclean.pages.dev",
  output: "static",
  integrations: [react(), sitemap()],
  vite: { plugins: [tailwindcss()] },
  build: { inlineStylesheets: "always" },
});
