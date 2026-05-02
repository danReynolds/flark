# Sovereign Release Checklist

Status date: 2026-05-02

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
generation dry run, example tests, host native build, native editor CI, full
package tests, and benchmark budgets.
