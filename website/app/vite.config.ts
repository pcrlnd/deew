import { resolve } from "node:path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"
import { viteSingleFile } from "vite-plugin-singlefile"

// https://vite.dev/config/
export default defineConfig({
  base: "./",
  plugins: [react(), tailwindcss(), viteSingleFile()],
  resolve: {
    alias: {
      "@": resolve(import.meta.dirname, "./src"),
    },
  },
  build: {
    // Write the single-file site next to app/ (website2/index.html).
    outDir: "..",
    emptyOutDir: false,
    // Big enough that the woff2 fonts become data URIs.
    assetsInlineLimit: 1024 * 1024,
  },
})
