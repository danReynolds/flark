# Flark docs site

Astro Starlight site. The **Using Flark** page presents the approved API design
before implementation; its snippets are design examples, not
executable examples of the current packages. Historical V4 guides remain in
`../docs` and are outside this site's navigation.

Requires Node.js 22.12 or newer and npm 9.6.5 or newer.

```sh
cd website
npm ci
npm run dev -- --port 4336
```

Open http://127.0.0.1:4336/guides/using-flark/ for browser annotations.
`npm run build` produces a static site in `dist`. No deployment is configured.

For the current implementation, use the [Flutter](../packages/flark_flutter/README.md)
and [Fleury](../packages/flark_fleury/README.md) package READMEs and runnable examples.
