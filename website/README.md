# deew website

Static project site for deew, built with Vite + React + TypeScript and the same
shadcn preset (`base-lyra`, zinc) as the GUI.

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

This writes `../index.html`, `../tool-chain.html` and shared hashed files in
`../assets/` (JS, CSS, fonts), and copies `app/public/*` next to them. Commit
all of them together with the source.

The pages must be served over HTTP (e.g. `npm run preview`, or
`python3 -m http.server` in `website/`); opening them from disk does not work.
