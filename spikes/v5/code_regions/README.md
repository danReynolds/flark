# Small code-region integration proof

**Decision: do not adopt this backend in Flark.** The generic indentation adapter
is feasible, but this particular TextMate integration fails the agreed simplicity
gate. See the [review](../../../docs/architecture/v5/code_region_spike_2026_09_06.md).

This standalone spike has no dependency from `packages/flark` or either host.
It does not change the current browser candidate. It uses `shiki_flutter` 1.1.0
with a pinned lockfile, Dart 3.12.2 and selected upstream language configurations.

## What is implemented

- `lib/lexical.dart`: exact UTF-16 lexical ranges and standard token categories
  from the existing TextMate runtime, including grammar state across blank lines.
  Raw scopes and the upstream standard-token classifier drive editing; themed
  token explanations do not. A theme change cannot change the lexical result.
- `lib/indentation.dart`: snippet-local edit proposals for Enter, paired-line
  splitting and typed bracket outdent. One generic matcher uses configured pairs;
  common Enter rules and increasing-indentation patterns come from upstream data.
- `bin/probe.dart`: explicit source/caret expectations after each action, lexical
  invariants, context changes and a small timing diagnostic.

The adapter deliberately uses unsupported `package:shiki_flutter/src/...` APIs
to answer the feasibility question. Those imports must not migrate to production.
The package dependency still requires Flutter and Dart 3.12, despite the engine
executing without Flutter widgets. Flark currently accepts Dart 3.10.4.

The full snippet is retokenized, with the existing 8,192 UTF-16-unit cutoff. The
1,000 ms tokenizer time-limit argument is an experimental escape path, not a
qualified keystroke budget or proof that a regex can be interrupted promptly.
No incremental cache, parser port, IDE service or second editor was implemented.

## Configuration scope and provenance

`upstream/` contains the original pinned configuration files, their retained
license notices and SHA-256 hashes. `tool/generate_configurations.py` reproduces
the selected data in `lib/configurations.dart` using only Python's standard
library. It verifies the pinned inputs before normalizing their inspected JSONC.

The runtime consumes single-character bracket pairs, `increaseIndentPattern`,
and Enter rules whose complete action is `indent`. Other indentation patterns
remain in the extracted data for inspection but are not executed. Comment text
continuation, `appendText`, `removeText`, full VS Code Enter precedence,
`indentNextLinePattern`, keyword reindentation and multi-character pairs are not
implemented. TypeScript uses the JS editing subset with its own TextMate grammar.
This is a bounded data-driven proof, not VS Code compatibility.

Language detection, language-picker UI, manual Tab/Shift-Tab, Markdown container
mapping, selection/history and platform input remain responsibilities of the
existing Flark implementation. The paste trace exercises literal insertion in
the probe harness; it does not validate browser clipboard transport.

## Results and reproduction

The final VM, native AOT and Dart-to-JavaScript runs have **identical 69 checks and
lexical snapshots**. Of those checks, **67 pass and two fail**: automatic Python
`else:` alignment and indentation after YAML `settings: |`. Those are visible
misses against desired behavior, not skipped checks or expected-pass exceptions.
The analyzer passes.

For a fresh replay, copy this spike to a disposable directory and run:

```sh
flutter pub get --enforce-lockfile
python3 tool/generate_configurations.py
dart analyze --fatal-infos
dart bin/probe.dart --bench > vm.json
dart compile exe bin/probe.dart -o probe-native
./probe-native --bench > aot.json
dart compile js bin/probe.dart -O2 -o probe.js
node -e 'globalThis.self = globalThis; globalThis.dartMainRunner = main => main(["--bench"]); require("./probe.js")' > js.json
```

Run the benchmarks sequentially. For a failing acceptance gate rather than an
observation report, add `--strict`: it prints the results and then throws if any
check fails. The saved two misses mean strict mode must currently fail.

`receipts/verification.json` identifies source inputs, machine, runtime versions,
result hashes, known misses and cross-runtime equality. `aot.json`, `js.json` and
`vm.json` contain all observations, construction/first-token costs, and median/
maximum timings over 12 warm complete-snippet tokenizations at each size. The
median averages the two central samples. The JS clock has millisecond granularity
in this Node execution, so small timings cannot establish sub-millisecond cost.

These measurements are diagnostic: repetitive ASCII fixtures, one developer
machine, no host layout/paint and no platform input. JavaScript ran under Node,
not a browser. Native AOT is a command-line executable, not a Flutter application.
Dart Wasm, device performance and pathological long-line limits were not
qualified. No Flark dogfood or release gate closes from this proof.
