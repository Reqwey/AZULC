# AZULC website

This branch contains only the Azusa Minecraft Launcher website. The Rust launcher source is on the [main branch](https://github.com/Reqwey/AZULC/tree/main).

## Local development

Requires Node.js 22.13 or newer.

```sh
npm ci
npm run dev
```

## Validation

```sh
npx tsc --noEmit
npm run build
```

The website uses React, Vinext, and the Sites Vite plugin. Page content lives in `app/page.tsx`, styles in `app/globals.css`, and document metadata in `app/layout.tsx`. Product screenshots and the original brand assets are in `public/`.

## Hosting

The existing Sites project is identified by `.openai/hosting.json`. Build output is generated in `dist/`. Keep local dependencies, build output, and credentials out of version control.

The site uses English throughout. Downloads link to the launcher's GitHub Releases; build instructions link to the launcher README on `main`.
