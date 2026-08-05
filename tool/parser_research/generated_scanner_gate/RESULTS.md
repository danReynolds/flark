# Generated resumable scanner result

Status: **two-family generation/cursor seam GO; complete scanner integration open**.

## What passed

- Pinned re2rust 4.3.1 plus rustfmt regenerates Comrak 0.54.0's complete
  `scanners.rs` byte-for-byte from its checked-in `scanners.re`.
- `generate.py` mechanically extracts the exact ATX accepting pattern and both
  `open_code_fence` accepting patterns from that same pinned source. The
  wrapper templates contain no handwritten Markdown regex.
- re2rust's storable-state mode generates an explicit resumable DFA from the
  extracted rule.
- Generated cursor artifacts use re2rust's generic API over a caller-owned,
  identity-checked byte cursor. They retain only absolute
  cursor/marker/context-marker/DFA state and **zero source bytes**; no complete
  physical line or refill buffer is owned by either scanner.
- 50,000 deterministic random physical lines match Comrak's actual
  `block_spine_facade::atx_heading_start` at grant sizes 1, 2, 3, 7, and 4,096.
- Focused EOF, CRLF, tab, overlong-marker, and no-space cases match at every
  tested grant size.
- A 1 MiB space run resumes across more than 200 polls while advancing at most
  4 KiB of source per poll.
- The cursor-backed artifact matches another 20,000 deterministic random lines
  at fuel 1, 2, 7, and 4,090. A 10 MiB ATX candidate completes across more than
  2,500 polls, inspects at most 4,096 source bytes in any poll, and still
  reports zero retained source bytes.
- Cursor/DFA state can be cloned at an arbitrary suspension and resumed against
  the same source identity under a different grant schedule with the same
  result and terminal cursor. Crossed source identity and an in-source sentinel
  fail closed.
- The generated fence cursor matches Comrak's actual
  `block_spine_facade::open_code_fence` on focused EOF, CRLF, Unicode,
  backtick-exclusion, and tilde cases at every tiny fuel. Another 20,000
  deterministic random lines match at fuel 1, 2, 7, and 4,090 and under a
  changing randomized fuel schedule.
- A fence scan cloned after suspension resumes under two different schedules
  to the same result, logical cursor, physical source high-water, and rewind
  receipt.
- Valid backtick, valid tilde, and invalid late-backtick 10 MiB trailing-context
  candidates each yield more than 2,500 times. Each exposes at least a 10 MiB
  logical restore while retaining zero source bytes and making **zero backwards
  source-byte requests**.
- The same generated cursor now runs directly over the real Crop-backed source
  shape through a strict sequential adapter. Tiny-fuel CRLF, tab, and Unicode
  cases still match the pinned Comrak facade. A 10 MiB candidate finishes in
  0.11 s in the release host test, with Crop copying at most 4 KiB per refill,
  a 16-byte test rewind cache, more than 2,500 yields, and no source retained by
  the DFA. This is a mechanism receipt, not a device SLA.
- A second, stricter Crop adapter runs all fence cases with **zero source-byte
  cache** and accepts only the exact next Crop byte. Across all three 10 MiB
  candidates, request count and first-read count equal the physical Crop cursor
  offset, maximum requested rewind is zero, and Crop never copies more than its
  fixed source-cursor chunk cap. The ATX adapter retains its existing 16-byte
  cache but now records the same first-read and requested-rewind evidence; its
  measured requested rewind is also zero.
- The fence scanner also clones mid-line and resumes against a strict adapter
  reconstructed from the same immutable Crop snapshot at recorded physical
  high-water. Different post-resume fuel schedules converge on the same result,
  logical cursor, physical high-water, and zero requested rewind.
- Tests, formatting, generator provenance, and Clippy with `-D warnings` pass.

The gate exposed one important generated-lexer invariant: final input needs
`YYMAXFILL` fake-sentinel padding, not one sentinel byte. That amount is emitted
by re2rust (`7` for this rule) through `/*!max:re2c*/`; it is not maintained as
a handwritten grammar constant. The cursor artifact virtualizes that padding
at logical EOF. re2rust also groups bounds checks around the longest
non-looping DFA path, so one poll has a generated hard lookahead slack of
`YYMAXFILL - 1`; its receipt counts actual source peeks rather than inferring
work from cursor movement.

The fence family exposes a second, independent invariant. Trailing context
forces the DFA to read through the physical line before it can decide whether
the opener is valid. re2rust then restores `YYCTXMARKER` (or `YYMARKER` on the
invalid path) so the returned logical match covers only the fence run. That
restore is terminal: after it, this generated function returns without reading
source again. Consequently the logical rewind can grow with the line while the
sequence of source-byte requests remains monotonic.

This does **not** invalidate a forward-only Crop source. It does invalidate two
shortcuts: `YYMAXFILL` cannot be treated as a bound on logical rewind, and the
scanner's returned/logical cursor cannot stand in for physical source progress.
The continuation contract must preserve scalar marker/context-marker state and
physical source high-water separately. Every additional scanner family still
needs the same instrumentation because a non-terminal restore followed by more
input could require a real rewind cache even though this fence rule does not.

## Why this changes the maintenance judgment

An oversized-line path no longer requires a separately handwritten lexical
grammar. A viable shipping seam can pin:

1. one Comrak `scanners.re` source and hash;
2. one re2rust version;
3. deterministic fixed and storable generated artifacts; and
4. generated-artifact plus donor-facade differentials in CI.

The remaining fork is then the small source-adapter/action wrapper and parser
continuation integration, rather than a second set of regex decisions.

## What remains open

- Generate every remaining block-significant family used by the exact block
  core, especially combined HTML starts, table rows, and reference prefixes.
- Move the passing strict Crop adapter behind the real ledger-owned recognition
  cursor. The current tests use query cursors, preserve ATX's conservative
  16-byte cache, and measure zero actual requested rewind for both families;
  production still needs typed source/build/line identity, exact-once
  decoder/digest observation, cancellation, and fail-closed rewind handling
  rather than a test assertion.
- Trace logical restore and actual source-request behavior for every selected
  generated scanner. `YYMAXFILL` bounds lookahead, not restore span. A family
  that rereads after an unbounded restore would require a different generated
  push machine, a bounded materialization strategy, or a fail-closed fallback;
  the fence result proves that a large logical restore alone does not require
  retained source.
- Extend actual-peek metering to DFA transitions and emitted output;
  cursor advance alone is still not a complete total-work receipt.
- Drain dense table/reference output through bounded event pages so lexical
  suspension is not defeated by a large allocation after acceptance.
- Add cancellation, restart/checkpoint, native/Wasm, and first upstream-forward
  port gates for the generated path.
- Pin the generator binary/toolchain supply chain in CI rather than relying on
  a workstation `/tmp` build.

This is therefore evidence that the scanner seam can stay architecturally
clean, not evidence that the full scanner integration is finished.
