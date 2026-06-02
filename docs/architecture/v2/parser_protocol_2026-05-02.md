# Flark v2 Parser Protocol

Status date: 2026-05-02

## Scope

The v2 parser protocol defines the Dart-side contract for authoritative
Markdown parse payloads before the native bridge is rewritten. The native Rust
bridge should eventually emit this schema.

## Current Contract

- Current schema version is `2`.
- Parse requests carry revision, source markdown, and markdown profile.
- Parser capabilities declare parser name, schema version, and supported
  profiles.
- Parse results carry schema version, revision, source text length, block
  nodes, inline tokens, hidden ranges, and diagnostics.
- Block and inline nodes preserve raw wire `type` strings.
- Hidden ranges identify source spans that projection should hide from display,
  such as inline markers, block markers, link destinations, reference
  definitions, and raw HTML.
- Ambiguity zones identify source spans where predictive projection may need an
  affinity choice until the parser returns authoritative structure.
- Unknown block/inline/hidden-range/ambiguity variants map to `unknown`,
  preserving the raw type.
- Unknown payload fields are preserved in `extensions` maps instead of
  crashing decode.

## Why This Matters

The v1 native bridge works, but its payload shape grew organically. v2 needs an
explicit schema so parser changes are contract-tested, forward-compatible, and
safe to adopt from Flutter adapters.

## Next Parser Work

- Add schema fixtures shared by Dart and Rust.
- Add native bridge v2 payload emission.
- Expand CommonMark/GFM fixture importers.
- Add UTF-8/UTF-16 source range contract tests.
