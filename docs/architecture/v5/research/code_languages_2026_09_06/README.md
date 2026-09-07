# Code language research probes

These artifacts support the [research recommendation](../../code_language_research_2026_09_06.md).
They are isolated dependency/mechanism probes. No Flark production dependency,
source file, browser preview or candidate receipt was changed by this research.

## Provenance

- `repositories.json`: inspected upstream revision and repository license metadata.
- `source_manifest.json`: pinned upstream source URLs, byte lengths and SHA-256
  hashes. Third-party implementation files are not vendored here.
- `dart_packages.json`: published versions, dependency metadata and archive hashes
  fetched from pub.dev on 2026-09-06. Published archives inspected locally were
  verified against their metadata hashes.
- Local execution: macOS arm64, Dart 3.12.2, Node 22.23.0. The Shiki dependency
  resolution used a Flutter SDK even though the probe imports only its engine.

## CodeMirror observations

Copy `codemirror/` to a disposable directory, then run there:

```sh
npm ci --ignore-scripts
node probe.mjs > reproduced-results.json
```

`package-lock.json` pins transitive packages. `results.json` contains 16
source/caret traces. The script exercises headless editor state and actual
language/indent commands; it does not instantiate a DOM editor. Each `type`
action is delivered as individual characters, while `paste` is one literal
transaction. Parser availability is forced before commands to avoid measuring
scheduling rather than semantics.

These are observations, not an assertion that every output meets Flark's desired
behavior. In particular, the sampled YAML mapping and shell `then` do not gain
indentation on Enter. No DOM, Flutter adapter, selection paint, IME, latency or
deployment claim follows from these traces.

## Dart TextMate observations

Copy `shiki/` to a disposable directory. With Dart 3.12.2 and a compatible Flutter
SDK available, run there:

```sh
flutter pub get --enforce-lockfile
dart run probe.dart > reproduced-vm-results.json
dart compile js probe.dart -O2 -o probe.js
node -e 'globalThis.self = globalThis; require("./probe.js")' > reproduced-js-results.json
```

`pubspec.lock` pins the resolved dependency versions. The two saved result files
have identical parsed JSON across six cases. Each run explicitly checks exact
input reconstruction from token content. `compile-js.log` records compilation
only; it is not a production bundle measurement or a runtime benchmark.

The deliberately sparse theme makes adjacent lexical tokens share a style. This
reveals why public themed tokens and their combined scope explanations do not
constitute precise lexical ranges for editing decisions. The probe does not
implement indentation or validate the underlying raw token API.

This executed JavaScript under Node, not a browser or Flutter web application.
Dart Wasm, native AOT, incremental token invalidation, long-line behavior and
typing latency remain unqualified. None of the probe results validates another
package merely because it uses TextMate or Tree-sitter.
