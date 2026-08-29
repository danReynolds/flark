# Flark documentation

Flark v4 has one active product path:

- [`flark`](../packages/flark/README.md) owns the headless Dart API
  over the Rust source and parser authority.
- [`flark_flutter`](../packages/flark_flutter/README.md) owns the Flutter editor
  and read-only rendering surfaces and re-exports the supported Dart API.

Start with [Getting started](getting_started.md), then use the
[API surface](api_surface.md), [Cookbook](cookbook.md), and
[Parser and platforms](parser_and_platforms.md) guides. Contributors should
also read [Development and verification](development.md) and
[Benchmarks](benchmarks.md).

The normative architecture lives under [`architecture/v4/`](architecture/v4/)
and in [RFC 027](architecture/rfc/rfc_027_continuously_rendered_markdown.md).
Superseded v2/v3 guides are preserved under [`legacy/docs/`](../legacy/docs/)
and do not define the active product or its release evidence.
