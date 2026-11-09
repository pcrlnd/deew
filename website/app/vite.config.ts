import { resolve } from "node:path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig, type Plugin } from "vite"
import { viteSingleFile } from "vite-plugin-singlefile"

// GitHub Pages serves /tool-chain for tool-chain.html; do the same in dev.
function devCleanUrls(): Plugin {
  return {
    name: "dev-clean-urls",
    apply: "serve",
    configureServer(server) {
      server.middlewares.use((req, _res, next) => {
        if (req.url === "/tool-chain") req.url = "/tool-chain.html"
        next()
      })
    },
  }
}

// vite-plugin-singlefile only supports one input per build, so each page is
// its own build: `vite build` makes index.html, `vite build --mode tool-chain`
// makes tool-chain.html (the build script runs both).
// https://vite.dev/config/
export default defineConfig(({ mode }) => {
  const toolChain = mode === "tool-chain"
  return {
    base: "./",
    // public/ is copied by the index.html build only.
    publicDir: toolChain ? false : "public",
    plugins: [react(), tailwindcss(), viteSingleFile(), devCleanUrls()],
    resolve: {
      alias: {
        "@": resolve(import.meta.dirname, "./src"),
      },
    },
    build: {
      // Write the single-file pages next to app/ (website/index.html, website/tool-chain.html).
      outDir: "..",
      emptyOutDir: false,
      // Big enough that the woff2 fonts become data URIs.
      assetsInlineLimit: 1024 * 1024,
      rollupOptions: {
        input: resolve(
          import.meta.dirname,
          toolChain ? "tool-chain.html" : "index.html"
        ),
      },
    },
  }
})
