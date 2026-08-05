# MD4C block-donor challenge

This disposable lane tests pinned MD4C as a **block orchestration donor**, not
as Flark's inline parser and not as a production C wrapper.

Pinned inputs:

- MD4C: `/tmp/flark-md4c-gate`, commit
  `65c6c9d72cebd9a731aaa5597414ce04d9ea5de3`.
- cmark-gfm: `/tmp/flark-cmark-gfm-gate`, commit
  `499789b49373bfa045d0e7547e5ee63444c77bca`.
- Exact inline/table/reference candidate: the bounded Comrak facades in the
  sibling research crates.

The probes include `md4c.c` only to reach its private block phase. That is
itself evidence: upstream MD4C does not expose a supported block-only API.

Build:

```sh
mkdir -p /tmp/flark-md4c-block-probe
cc -O2 -std=c11 -Wall -Wextra \
  -I /tmp/flark-md4c-gate/src \
  tool/parser_research/md4c_block_donor_challenge/block_probe.c \
  -o /tmp/flark-md4c-block-probe/block_probe
cc -O2 -std=c11 -Wall -Wextra \
  -I /tmp/flark-md4c-gate/src \
  tool/parser_research/md4c_block_donor_challenge/full_parse_probe.c \
  /tmp/flark-md4c-gate/src/md4c.c \
  -o /tmp/flark-md4c-block-probe/full_parse_probe
cc -O2 -std=c11 -Wall -Wextra \
  -I /tmp/flark-md4c-gate/src \
  tool/parser_research/md4c_block_donor_challenge/checkpoint_probe.c \
  -o /tmp/flark-md4c-block-probe/checkpoint_probe
```

`block_probe` runs the internal line/block pass without MD4C's inline/render
pass, can dump its flat block tape, and can poll cancellation between physical
lines. `full_parse_probe` measures stock whole-document `md_parse` with no-op
callbacks. `source_audit.py` reports conservative `md_*` dependency closures
at the candidate seams. `checkpoint_probe` deep-clones and rebases the private
context to distinguish resumable grammar state from a production-quality
persistent checkpoint. `terminal_break_audit.py` checks the eight nested-list
rows that the HTML canonicalizer originally misclassified as inline
softbreaks.

Run that correction receipt against the built vendored Comrak CLI:

```sh
python3 tool/parser_research/md4c_block_donor_challenge/terminal_break_audit.py \
  --spec /tmp/flark-cmark-gfm-gate/test/spec.txt \
  --comrak tool/parser_research/comrak_inline_fragment_gate/target/debug/comrak
```

See [RESULTS.md](RESULTS.md) for receipts and the recommendation.
