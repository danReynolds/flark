# Sourcepos and derivation register

State at M1 exit (2026-09-02): the conformance differential over 1,322
upstream cases reports **zero deviations**. Every class found during the
spikes now has a deterministic correction inside the extraction, validated
against comrak's own output.

| Class | Cases in the corpora | Correction |
| --- | --- | --- |
| Partial tab expansion in container indentation and indented code | 6 | `virtual_leading_spaces` on the content record; hosts render that many spaces before the line |
| Escaped pipe `\|` in table cells shifts inline positions | 1 | positions mapped back through the raw cell text; code spans display the unescaped literal via the string table |
| Inline positions after definitions comrak stripped from a paragraph or setext heading | 12 | comrak's `resolve_reference_link_definitions` mirrored over the leaf's content buffer; positions remapped by line |
| One-byte block sourcepos for indented code inside containers | 8 | block range widened to its validated content, ancestors with it |
| Closing fence inside a container; nested `> 1. >` chains; HTML block first-line indentation | fixed | scanner derivation fixes |

Anything new fails `cargo test` in this crate.
