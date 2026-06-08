---
id: recipes/build-docs-site
title: Build Docs Site
shard: 10-recipes
tags: [docs, mcp, starlight, mdbook]
summary: Cómo usar la capacidad docs.build para convertir Markdown del repo en un sitio estático.
related: [agents/smart-loading, build-plan/pending-implementation-tasks]
sources: []
---

# Build Docs Site

`docs_build` es la tool MCP que implementa la capacidad conceptual `docs.build`.
Toma Markdown del workspace y crea un sitio estático en un directorio generado.

Uso típico:

```json
{
  "source_dir": "docs",
  "output_dir": "docs-site",
  "backend": "auto",
  "title": "Project Docs"
}
```

Backends:

- `auto`: usa `mdbook` solo para repos Rust puros; si no, usa Starlight.
- `starlight`: genera `package.json`, `astro.config.mjs` y copia Markdown a `src/content/docs/`.
- `mdbook`: genera `book.toml`, `src/` y `dist/`.
- `vitepress`: genera scaffold mínimo con `docs/.vitepress/config.mts`.

Por defecto la tool no instala dependencias Node. Si `node_modules` no existe,
devuelve `build_ran=false` con una razón accionable. Para instalar desde la tool,
pasar `"install": true`; esto ejecuta `pnpm install` dentro de `output_dir`.

Reglas:

- `source_dir` y `output_dir` son workspace-relative.
- No se permiten rutas absolutas ni `..`.
- `source_dir` y `output_dir` no pueden solaparse.
- La tool escribe archivos, así que pasa por policy/approval como capacidad sensible.
