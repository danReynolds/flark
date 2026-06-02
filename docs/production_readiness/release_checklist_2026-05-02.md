# Flark Release Checklist

Status date: 2026-05-31

This checklist records the release decision boundary while the package remains
private with `publish_to: none`.

## Required Before Publishing

- Choose and add a `LICENSE` file. This is an owner/legal decision and must not
  be guessed during engineering hardening.
- Choose canonical public URLs before adding pub metadata:
  - `repository`
  - `issue_tracker`
  - `documentation`
  - optional `homepage`
- Decide whether external pub.dev screenshots are needed. They are deferred
  while the package is private and unpublished.
- Remove or update `publish_to: none` only when public publishing is intended.

## Release Dry Run

When release intent is explicit and metadata/license decisions are complete,
run:

```bash
dart pub publish --dry-run
```

Keep this separate from `scripts/verify_release.sh` while `publish_to: none`
remains in `pubspec.yaml`; the release gate should verify package health without
pretending the package is publishable.

## Current Release Gate

Run:

```bash
./scripts/verify_release.sh
```

The gate covers dependency resolution, package and example analysis, docs
generation dry run, example tests, promoted-widget web smoke coverage, host
native build, native editor CI, full package tests, the curated v2 visual
golden suite, and benchmark budgets.

Latest result on 2026-05-31: passed after the v2 release-hardening pass,
including source-first image card actions and code-fence copy actions. The gate
covered package/example dependency resolution, package and example analysis,
`dart doc --dry-run` with 0 warnings and 0 errors, example tests, web smoke
coverage, host native Comrak build, native editor CI, full package tests
including visual goldens, and enforced benchmark budgets. The web smoke includes
the packaged Comrak WASM backend. External publishing remains blocked only on
the owner decisions listed above.

## Optional Device Dogfood Gate

Run this when an iOS simulator is available and example behavior changes:

```bash
cd example
flutter test integration_test/markdown_flow_test.dart -d <ios-simulator-id>
```

The current simulator validation was run against
`C880BCBA-DC57-4AB9-87DA-50A44357BC40`. It exercises the v2 example app through
common Markdown cases, source/projected/live-rendered mode switching, and
native parser adoption.

Run this on macOS when desktop behavior or native loading changes:

```bash
cd example
flutter test integration_test/markdown_flow_test.dart -d macos
```

The macOS dogfood app can be launched manually with:

```bash
cd example
flutter run -d macos
```
