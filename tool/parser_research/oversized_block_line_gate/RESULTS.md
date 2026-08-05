# Oversized physical-line gate results

## Verdict

**GO on the architecture mechanism; STOP on shipping this implementation.**

An exact block spine does not need to make an 8 KiB/large-line choice between
Comrak and a predictive fallback. The whole-line-sensitive scanner families can
retain constant-size lexical state, yield every 4 KiB, serialize that state,
and resume without changing downstream block continuation. Giant constructs can
therefore remain exact and source-visible.

The handwritten machines in this crate are not an acceptable grammar
authority. They exist to establish that the states and scheduling contract are
possible and to expose non-obvious donor semantics. The shipping gate must do
one of the following:

1. use one generated resumable scanner for every input size; or
2. generate fixed-slice and refillable scanners from the same pinned scanner
   definition and prove them equivalent.

An ordinary Comrak scanner below 8 KiB plus a separately handwritten scanner
above 8 KiB is a **STOP** outcome even if all current differentials pass.

## What the witness covers

All of these jobs are invoked at every size in this gate:

| Family | State/output proved |
| --- | --- |
| backtick/tilde fence open and close | marker, run, remainder/whitespace validity |
| ATX start and trailing closure | opener consumption and forward trailing-hash fold |
| setext and thematic marker lines | marker/count/full-line validity |
| HTML block ends 1-5 | resumable terminator search |
| HTML start type 7 | tag/attribute/value DFA |
| GFM table row and delimiter | source ranges, offsets, escaped-pipe behavior, cell cap, alignments |
| leading reference definition | bounded label, URL nesting/escaping, SPNL, title/fallback, exact source ranges |

The differentials include 20,000 randomized ordinary physical lines for the
fixed scanners, 30,000 HTML type-7 lines, 20,000 table rows, and 35,000
reference-definition candidates. Each is repeated with different fuel sizes.
The donor's unusual semantics are preserved, including table pipes following
backslash runs and title metadata retained across a reference-definition cursor
fallback.

Five 1 MiB downstream scenarios also compare against pristine Comrak block
projections: giant fence close, HTML close, reference removal, table activation,
and invalid giant constructs followed by a normal paragraph.

Not yet proved here:

- generation of the resumable jobs from the scanner source;
- HTML start types 1-6 as a generated combined scanner (their lexical decision
  has a finite prefix, but that does not grant a second authority);
- spoiler-table profile behavior;
- container-prefix/list-marker scanning, which is prefix-bounded but belongs in
  the same generated/provenance gate;
- streamed entity/unescape normalization for giant fence info, URL, or title;
- a production chunked output sink for maximum-cell table rows.

## Large-line receipts

Release build on the local development machine, fixed 4,096-byte poll grant:

| Classifier | 1 MiB total | 10 MiB total | 10 MiB polls | Max bytes/poll |
| --- | ---: | ---: | ---: | ---: |
| fence open, backtick remainder | 1.07 ms | 11.14 ms | 2,560 | 4,096 |
| fence close, whitespace tail | 1.23 ms | 13.37 ms | 2,560 | 4,096 |
| setext/thematic | 1.06 ms | 12.30 ms | 2,560 | 4,096 |
| ATX tail | 1.10 ms | 13.38 ms | 2,560 | 4,096 |
| HTML comment terminator | 2.14 ms | 25.57 ms | 2,560 | 4,096 |
| HTML type 7 | 1.06 ms | 11.64 ms | 2,560 | 4,096 |
| table, two cells | 2.60 ms | 27.03 ms | 2,560 | 4,096 |
| reference definition | 3.51 ms | 47.85 ms | 2,561 | 4,096 |

These totals are continuous throughput measurements, not a proposal to occupy
the UI thread for the total duration. Every job returns after at most 4,096
inspected bytes; cancelling after one grant was observed before any additional
byte was inspected.

Two dense-table adversaries challenge output work separately:

| Shape | Input | Inspected | Output | Measured max poll |
| --- | ---: | ---: | ---: | ---: |
| exactly 65,535 cells | 10 MiB | 10 MiB | 3,145,776 bytes | 206 us |
| 65,536th cell | 10 MiB | 131,072 bytes | none | 50 us |

The scanner can reject permanently once the donor's cell cap is crossed. The
valid maximum-cell row exposes a separate concern: contiguous `Vec` growth was
small in this run, but is not a hard scheduling guarantee. Production must emit
cell records into bounded chunks/a drainable sink; lexical yielding alone does
not make output allocation realtime-safe.

## Ordinary-size cost

Median release microbenchmarks, 50,000 operations per trial:

| Classifier | Resumable / current facade |
| --- | ---: |
| fence open | 1.04x |
| HTML type 7 | 1.09x |
| table structural row | 0.41x |
| reference structural shape | 0.30x |

Fence and HTML are useful like-for-like signals: making the state resumable did
not reveal an inherent ordinary-line tax. Table and reference are **not**
like-for-like performance claims: the facade also materializes/normalizes owned
strings, while the witness returns source-backed structural ranges. The
generated scanner gate must repeat this measurement with equivalent output and
an end-to-end block transition workload.

## Generator and maintenance finding

The official re2rust manual documents `--storable-state` plus `YYFILL` for a
push lexer that stores its DFA state and resumes when more input arrives:
<https://re2c.org/manual/manual_rust.html#storable-state>.

Generator version is grammar provenance. The vendored Comrak
`src/scanners.rs` says it was generated by re2rust 4.3.1. The current official
release is 4.5.1, and re2c 4.4 changed end-of-input rule precedence:
<https://re2c.org/releases/release_notes.html>. The first generator gate should
therefore pin 4.3.1 to match the donor artifact. A move to 4.5.x is a separately
adjudicated grammar change, not an incidental tool upgrade.

## Next gate

1. Share the selected rules/definitions directly from the pinned Comrak
   `scanners.re` lineage.
2. Generate a fixed-slice oracle artifact and a storable-state/refill artifact
   with a locked generator fingerprint.
3. Differential the fixed artifact against current Comrak, then the refill
   artifact against the fixed artifact on the full corpus and randomized bytes.
4. Pause, serialize, restore, and resume at every possible split for bounded
   cases, plus randomized 1/10 MiB chunk schedules.
5. Run the resumable artifact at ordinary sizes and require both an absolute
   latency budget and an agreed relative regression ceiling.
6. Keep checkpoint equality semantic: DFA state, cursor/origin state, and
   bounded lookbehind only; never output tree or giant input clones.
7. Drain source-backed output incrementally and separately account allocation,
   normalization, and cancellation.
8. Pin generated output/fingerprints in CI and fail on scanner-source,
   generator-version, or artifact drift.

Stop and reconsider the donor/generator route if exact refill semantics require
handwritten grammar corrections, if checkpoint state grows with the lexeme, if
ordinary-line cost misses the product budget, or if output/normalization cannot
be made independently yieldable.
