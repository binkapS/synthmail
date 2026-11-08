import adapterStatic from "@sveltejs/adapter-static";
import { sveltekit } from "@sveltejs/kit/vite";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [
    tailwindcss(),
    sveltekit({
      adapter: adapterStatic({
        pages: "build",
        assets: "build",
        fallback: "index.html",
        precompress: false,
        strict: true
      })
    })
  ],
  server: { proxy: { "/api": "http://localhost:8000", "/jmap": "http://localhost:3000" } }
});
