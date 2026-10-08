# inplace-init documentation

This directory contains the English-first, English and Simplified Chinese documentation site for `inplace-init`, built with Astro and Starlight. English is served at `/`; Chinese pages are served under `/zh-cn/`.

## Work on the docs

```sh
pnpm install
pnpm dev
pnpm build
pnpm preview
```

Add English pages under `src/content/docs/` and their Simplified Chinese counterparts at the same path below `src/content/docs/zh-cn/`. Keep the page slugs paired so Starlight can preserve the current page when a reader changes languages. Update the sidebar in `astro.config.mjs` when adding pages.

## 文档站开发

本目录使用 Astro + Starlight 构建 `inplace-init` 的英文优先中英双语文档。英文页面位于 `/`，简体中文页面位于 `/zh-cn/`。

```sh
pnpm install
pnpm dev
pnpm build
pnpm preview
```

英文页面放在 `src/content/docs/`，对应的简体中文页面放在 `src/content/docs/zh-cn/` 下的相同相对路径。新增页面时请保持两种语言的 slug 对应，并更新 `astro.config.mjs` 中的侧边栏。
