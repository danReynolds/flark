# Flark homepage and docs

Astro Starlight site. The homepage introduces the current packages and includes
a full-page Flutter composer built from the repository's actual Flark package.
Visitors can edit, format, undo, inspect source, copy Markdown, and reset the
sample. Welcome, meeting notes, and a blank page have independent editing sessions;
switching tabs preserves each document and its undo history. The introduction stays
compact, and the composer fills the rest of the page. Edits are kept in memory for
that page session, with no autosave.
The **Using Flark** page documents the implemented consumer API. The homepage
uses that API, and both hosts have minimal `consumer.dart` entry points plus
behavioral tests. Historical V4 guides remain in
`../docs` and are outside this site's navigation.

Requires Node.js 22.12 or newer, npm 9.6.5 or newer, and Flutter 3.44.4 / Dart 3.12.2
on PATH. `FLUTTER` can point to a different Flutter executable.

```sh
cd website
npm ci
npm run dev -- --port 4336
```

Open http://127.0.0.1:4336/ for the homepage, or
http://127.0.0.1:4336/guides/using-flark/ for the API design.

`dev` and `build` first compile `playground/` and bundle it under a content-derived
asset path. Generated demo files are ignored by Git. For site-only edits after
that first build, use `npm run dev:site -- --port 4336`; after Dart edits, rebuild
with `npm run build:playground`.

The iframe loads as it approaches the viewport, starts without taking keyboard
focus, follows the page theme without replacing the document, and exposes a
retry action if startup fails. Its scripts, parser Wasm, and Flutter renderer
are served with the site; no separate playground server is required.

Check the demo with `cd playground && flutter analyze && flutter test`.

`npm run build` produces the static site in `dist` with the `/flark` base path
for https://danreynolds.github.io/flark/. To check that deployment layout locally:

```sh
npm run build
npm run preview -- --port 4337
```

Open http://127.0.0.1:4337/flark/. Local development keeps the root URL so existing
annotation links continue to work. Relative guide links and generated navigation
work at both paths.

The **Flark homepage and docs** GitHub Actions workflow publishes the site to
the repository's existing GitHub Pages environment when manually dispatched from
`main`. It does not build or publish the Flark package examples. Publishing the
site is separate from building it locally.

For the current implementation, use the [Flutter](../packages/flark_flutter/README.md)
and [Fleury](../packages/flark_fleury/README.md) package READMEs and runnable examples.
