# Flark documentation

Read the approved widget/controller API design in the
[Using Flark guide](../website/src/content/docs/guides/using-flark.mdx).
Its APIs are not implemented yet. The [docs site](../website/README.md) serves
the proposal locally for annotation. For current integration, use the
[Flutter](../packages/flark_flutter/README.md) and
[Fleury](../packages/flark_fleury/README.md) package READMEs and examples.

Flark V5 is the active development path. For architecture, start with
[RFC 030](architecture/rfc/rfc_030_synchronous_core.md), the
[V5 build plan](architecture/v5/build_plan.md), and the
[North Star](../NORTH_STAR.md). The current code is the headless synchronous
kernel in `packages/flark`, the Flutter host in `packages/flark_flutter`, and
the Fleury host in `packages/flark_fleury`. Each host includes a runnable example.

The older [Getting started](getting_started.md), [API surface](api_surface.md),
[Cookbook](cookbook.md), [Parser and platforms](parser_and_platforms.md),
[Development](development.md), and [Benchmarks](benchmarks.md) guides describe
the archived V4 surface. They remain implementation evidence, not current V5
instructions, until their corresponding V5 surfaces exist.
