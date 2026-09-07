# AZULC website

The `site` branch contains only the Azusa Minecraft Launcher website. The Rust launcher is on the [main branch](https://github.com/Reqwey/AZULC/tree/main).

## Local development

Use Node.js 22.16.0, as pinned in `.node-version`. This version is verified for the static export on Windows and used by Pages.

```sh
npm ci
npm run dev
```

## Static build

```sh
npx tsc --noEmit
npm run build
npm run preview
```

Vinext exports the complete website to `dist/client`, including `index.html`, `404.html`, CSS, browser JavaScript, fonts, and screenshots. Only this directory is deployed. No Worker, server runtime, API keys, or database bindings are needed. Animations run in the browser.

## Cloudflare Pages

Connect the GitHub repository and use these build settings:

| Setting                    | Value                              |
| -------------------------- | ---------------------------------- |
| Production branch          | `site`                             |
| Framework preset           | None                               |
| Root directory             | Leave empty (repository root)      |
| Build command              | `npm run build`                    |
| Build output directory     | `dist/client`                      |
| Node version               | `22.16.0` (set in `.node-version`) |
| Preview branch deployments | None, to build only `site`         |

Commit and push website changes to `site`; Pages installs dependencies and builds automatically. Do not deploy `dist/server` or the entire `dist` directory. You do not need a Worker deploy command or the Next.js Pages adapter.

For a local check, `npm run preview` serves the exported files. Use the local URL printed by Vite.

## Source layout

- `app/page.tsx`: English page content.
- `app/globals.css`: styling and CSS animations.
- `app/motion.tsx`: progressive scroll animations.
- `app/layout.tsx`: document metadata.
- `public/`: product screenshots, fonts, and original brand assets.

Downloads link to GitHub Releases. Launcher build instructions link explicitly to the README on `main`.

The existing optional Sites project ID is preserved in `.openai/hosting.json`, with its static directory set to the same `dist/client` output. Cloudflare Pages does not use that file. Build output and local credentials stay out of Git.
