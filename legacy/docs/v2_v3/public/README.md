# Flark Documentation

Start here for package-level docs:

- [Getting Started](getting_started.md): build an editor, preview Markdown, and
  share state between surfaces.
- [Cookbook](cookbook.md): copy-pasteable recipes for toolbars, forms, save
  state, link editing, document switching, and custom previews.
- [API Surface](api_surface.md): which import and type to use for common app,
  core, and advanced integration work.
- [Parser and Platforms](parser_and_platforms.md): native, web, Comrak, and
  custom parser behavior.
- [Development and Verification](development.md): local test, docs, example,
  native, and release gates.

Current planning docs:

- [Flark v3 Definitive Architecture Summary](architecture/v3/architecture_summary.md):
  implementation baseline, Dart-first package boundary, runtime ownership,
  and production gates.
- [Flark v3 Production Implementation Plan](architecture/v3/implementation_plan.md):
  active milestone sequence, acceptance gates, and current implementation
  state.
- [RFC 023: Incremental Live Markdown Engine](architecture/rfc/rfc_023_incremental_live_markdown_engine.md):
  full large-document engine rationale, behavioral inheritance gates,
  alternatives, and evidence chain.
- [DX and Ergonomics Peer Audit](architecture/flark/dx_ergonomics_peer_audit_2026-06-05.md):
  where Flark stands against Flutter editor peers and what app-facing DX work
  should come next.
- [DX Confidence Peer Loop](architecture/flark/dx_confidence_peer_loop_2026-06-07.md):
  current seven-plus-loop peer comparison and confidence conclusion.

The rest of this directory is the design and release archive. Those documents
are useful when changing Flark internals, but app integrations should not need
them for routine usage.
