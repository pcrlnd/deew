import { readFileSync, rmSync } from "node:fs"
import { resolve } from "node:path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig, type Plugin } from "vite"

// The deew version comes from the repo root Cargo.toml ([package] version).
function deewVersion(): string {
  const toml = readFileSync(
    resolve(import.meta.dirname, "../../Cargo.toml"),
    "utf8"
  )
  const m = toml.match(/^version\s*=\s*"([^"]+)"/m)
  if (!m) throw new Error("Could not find the version in ../../Cargo.toml")
  return m[1]
}

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

// Build only: remove website/assets before each build so stale hashed files
// don't pile up. Deletes that one folder and nothing else.
function cleanAssets(): Plugin {
  return {
    name: "clean-assets",
    apply: "build",
    buildStart() {
      rmSync(resolve(import.meta.dirname, "../assets"), {
        recursive: true,
        force: true,
      })
    },
  }
}

// Build only: preload the two Latin fonts in both pages. `font-display: block`
// hides text until its font arrives, and the preload starts the download
// before the CSS is parsed. Throws if a font is missing (e.g. renamed).
function preloadFonts(): Plugin {
  const wanted = [
    "ibm-plex-sans-latin-wght-normal",
    "ibm-plex-mono-latin-400-normal",
  ]
  return {
    name: "preload-fonts",
    apply: "build",
    transformIndexHtml: {
      order: "post",
      handler(_html, ctx) {
        const files = Object.values(ctx.bundle ?? {})
          .filter((o) => o.type === "asset")
          .map((o) => o.fileName)
        return wanted.map((prefix) => {
          const file = files.find((f) => {
            const base = f.split("/").pop() ?? ""
            return base.startsWith(prefix + "-") && base.endsWith(".woff2")
          })
          if (!file) throw new Error(`preload-fonts: no emitted font ${prefix}`)
          return {
            tag: "link",
            attrs: {
              rel: "preload",
              as: "font",
              type: "font/woff2",
              crossorigin: "",
              href: "./" + file,
            },
            injectTo: "head" as const,
          }
        })
      },
    },
  }
}

// One multi-page build: index.html and tool-chain.html share hashed chunks in
// ../assets/ (React, shared components, CSS, fonts).
// https://vite.dev/config/
export default defineConfig({
  base: "./",
  define: { __DEEW_VERSION__: JSON.stringify(deewVersion()) },
  publicDir: "public",
  plugins: [
    react(),
    tailwindcss(),
    devCleanUrls(),
    cleanAssets(),
    preloadFonts(),
  ],
  resolve: {
    alias: {
      "@": resolve(import.meta.dirname, "./src"),
    },
  },
  build: {
    // Write the pages next to app/ (website/index.html, website/tool-chain.html).
    outDir: "..",
    emptyOutDir: false,
    assetsDir: "assets",
    // Keep the woff2 fonts as separate, cacheable files.
    assetsInlineLimit: 0,
    rollupOptions: {
      input: {
        index: resolve(import.meta.dirname, "index.html"),
        "tool-chain": resolve(import.meta.dirname, "tool-chain.html"),
      },
    },
  },
})
