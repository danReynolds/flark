# Provenance

This experimental crate is Flark-owned code derived from the algorithmic
structure of `pulldown-cmark` 0.13.4, especially:

- delimiter flanking and the modulo-three emphasis rule in
  `src/firstpass.rs` and `InlineStack` in `src/parse.rs`;
- the precedence order for code spans, links, and emphasis in `src/parse.rs`;
- inline-link destination/title rules in `src/parse.rs` and `src/scanners.rs`.

No `pulldown-cmark` source file is copied wholesale. Names, state layout, input
model, suspension protocol, and output model were rewritten for this spike.
The original MIT license is preserved verbatim in `PULLDOWN_CMARK_LICENSE`.
The clean differential tests use `pulldown-cmark = 0.13.4` as a development
oracle and do not link it into the library.

The central experiment is deliberately representation-changing: it replaces
Pulldown's mutable `Tree<Item>` with a compact lexical tape, value-only stacks,
and direct source facts. A successful experiment is not evidence that the rest
of CommonMark is implemented; it only falsifies the claim that these inline
algorithms intrinsically require `TreeIndex` and tree surgery.
