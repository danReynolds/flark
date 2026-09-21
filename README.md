# Flark

**[Homepage](https://danreynolds.github.io/flark/)** ·
[Flutter](packages/flark_flutter/README.md) ·
[Fleury](packages/flark_fleury/README.md)

Flark V5 is a live Markdown editor with one Dart editing kernel and two hosts:
Flutter and Fleury. Exact Markdown source stays canonical while the editor
projects rendered text, formatting, tables and code regions with editable source
anchors. Documents outside the configured live-rendering limits use source mode.

| Package | Responsibility |
| --- | --- |
| [`flark`](packages/flark) | Source, selection, commands, projection, composition transactions and undo; synchronous Comrak parse transport through native FFI or Wasm |
| [`flark_flutter`](packages/flark_flutter/README.md) | Flutter input, pixel layout, painting, accessibility semantics, controls and image previews |
| [`flark_fleury`](packages/flark_fleury/README.md) | Terminal/browser input, cell layout, painting, controls and image previews |
| [`flark_tree_sitter`](packages/flark_tree_sitter/README.md) | Shared code highlighting, language detection and indentation services |

Both host packages include a runnable example with a theme panel and one live
editor. Their READMEs cover setup, native/web builds and customization.

The [Using Flark guide](website/src/content/docs/guides/using-flark.mdx) records
the widget/controller API used by the homepage composer.
Run the [docs site locally](website/README.md) to read it and leave annotations.

```dart
import 'package:flark_flutter/flark_flutter.dart';

FlarkEditor(initialMarkdown: '# My note', onChanged: saveMarkdown);
FlarkMarkdown(markdown: article, selectable: true);
```

No parser setup or global initialization. Use `FlarkController(markdown: ...)`
for programmatic editing, observable formatting state and undo. Fleury exposes
the same component names and common controller API from its host package.

## Status and supported limits

The Flutter, Flutter-web/Wasm and Fleury implementations are merged. They are
development packages (`publish_to: none`), not a qualified tagged release. The
Fleury dependency is pinned to a reviewed, merged Git revision; no local path
override is required for that framework.

The core defaults to a 16 KiB UTF-8 live limit with additional line, block, run
and nesting limits. The Flutter workbench's 32 KiB live / 256 KiB source profile
is a qualification candidate. The core's configurable 1 MiB source ceiling is
an acceptance limit, not a universal latency guarantee. Browser, terminal,
native OS input and device evidence are tracked separately.

Current evidence and remaining release gates:

- [Release readiness](website/src/content/docs/guides/release-readiness.md)
- [Consumer API implementation review](docs/architecture/v5/consumer_api_review_2026_09_21.md)
- [Markdown coverage](docs/architecture/v5/markdown_coverage_2026_09_20.md)
- [Attended native results](docs/architecture/v5/native_attended_2026_09_20.md)
- [Production audit](docs/architecture/v5/production_audit_2026_09_16.md)
- [Fleury support closeout](docs/architecture/v5/fleury_support_closeout_2026_09_16.md)
- [macOS qualification and limits](docs/architecture/v5/macos_qualification.md)
- [V5 implementation plan](docs/architecture/v5/build_plan.md)

## Development checks

Run `dart analyze` and `dart test` from `packages/flark`,
`packages/flark_tree_sitter` and `packages/flark_fleury`. Run `flutter analyze`
and `flutter test` from `packages/flark_flutter` and its example. The Fleury
example uses `dart analyze` and `dart test`.

```sh
cargo test --release --manifest-path native/flark_parse/Cargo.toml
bash native/flark_parse/tool/verify_transports.sh --rebuild
bash packages/flark/tool/verify_prebuilt_consumer.sh
```

`packages/flark/tool/bench_editor.dart` measures the kernel facade;
`packages/flark_fleury/tool/perf_audit.dart` measures native input-to-cell-buffer
CPU work. Neither substitutes for actual input-to-presentation qualification.

Read the [North Star](NORTH_STAR.md) and
[live editor test strategy](docs/testing/live_editor_test_strategy.md) before
changing editing behavior or its tests. V4 sources and benchmark receipts under
`legacy/` and `benchmark/v4/` are historical and do not qualify V5.
