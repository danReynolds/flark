# Development and Verification

## Package Setup

```bash
dart pub get
cd packages/flark_flutter && flutter pub get
```

## Fast Gate

```bash
./scripts/verify_package_confidence.sh
```

This runs analysis and the highest-signal editor, parser, projection,
render-plan, Flutter, example, native, and packaging tests.

## Full Release Gate

```bash
./scripts/verify_release.sh
```

This is the release-readiness gate for package maintainers.

To inspect both publish archives without publishing them:

```bash
./scripts/verify_publish_archives.sh
```

The script validates the nested `flark_flutter` package from an isolated Git
discovery boundary. The repository root is the published `flark` package and
intentionally excludes `packages/` from its own archive.

## Docs

```bash
dart doc --dry-run
```

The dry run should report zero warnings and zero errors before public API
changes are considered complete.

## Example App

```bash
cd example
flutter run -d macos
```

The example app is the primary manual QA surface for live-rendered editing,
toolbars, source mode, preview rendering, parser load status, forms, and
scratch-pad workflows.

## Native Builds

```bash
./scripts/build_comrak_all.sh --host-only
./scripts/build_comrak_all.sh --strict
```

Use the strict build before changing native bridge ABI or packaging behavior.
