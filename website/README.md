# deew website

Static project site for deew, built with Vite + React + TypeScript and the same
shadcn preset (`base-lyra`, zinc) as the GUI. The built `index.html` is a single
self-contained file (JS, CSS and fonts inlined), so it can be opened directly
from disk.

## Develop

```sh
cd app
npm install
npm run dev
```

## Build

```sh
cd app
npm run build
```

This writes `../index.html` (and copies `app/public/*` next to it). Commit the
result together with the source.
