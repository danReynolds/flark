# Gate A harness

This is an executable acceptance contract for the first RFC 023 parser gate,
not another parser prototype.

The candidate implements \`GateAEngine\`. The runner then requires:

- exact normalized HTML for all 189 CommonMark/GFM block fixtures covering
  tabs, setext headings, all seven HTML block classes, quotes, list items,
  lists, and tables;
- candidate-clean equality at every revision of focused typing, erasing, and
  ambiguity histories, with Comrak and Pulldown recorded as independent
  differential evidence rather than made normative;
- total UTF-8 source coverage, exact source facts, stable IDs, and direct
  chunk/fact deltas that replay independently to the committed snapshot;
- acyclic, source-containing semantic parentage that also matches the clean
  parse rather than merely pointing at an existing ID;
- coverage pages bounded to 64 KiB even when one semantic leaf spans many
  megabytes, so a mutable whole-document record cannot fake locality;
- poll receipts bounded by byte and transition fuel, including 10 MiB physical
  paragraph, HTML-comment, and table-row lines;
- a global HTML-comment activation edit that legitimately reclassifies more
  than a megabyte while still requiring a compact sequence delta and stable
  identity outside the blast radius;
- explicit memory accounting under a 64 MiB general persistent-state cap and
  a stricter 16 MiB auxiliary-state cap for the two-million-byte,
  million-line fixture, so token/checkpoint-per-newline designs fail;
- 10,000 same-boundary insertions and a deterministic 100,000-edit bounded
  random history without order-label exhaustion; and
- zero batch-tree materializations and zero grammar-sensitive side scans on
  the delta path.

Run the harness self-tests and oracle pin with:

\`\`\`sh
cargo test --release
\`\`\`

The self-tests prove that the contract rejects the failures already observed
in both prototypes. They do **not** claim Gate A passes: that requires wiring
the persistent candidate core to \`GateAEngine\` and running the expensive
resource/history lanes in isolated subprocesses with independent RSS and wall
time measurement.

\`open\`, \`snapshot\`, normalized HTML, and the clean oracle are explicitly
test-only batch views. They are not liveness receipts. On the production edit
path, \`begin_edit\` and \`commit\` return phase receipts and must perform zero
grammar work, no batch-tree conversion or side scan, at most 64 bytes of source
inspection, and bounded structural/allocation work; all grammar work belongs to
fuelled \`poll\` calls. Final acceptance additionally instruments allocator
traffic, process RSS, and wall time outside the candidate, because self-reported
resource counters alone are not proof.

The candidate contract is donor-neutral. A Pulldown-derived, Comrak-derived,
or clean-room implementation gets exactly the same fixtures, revision
histories, output contract, and resource gates. Normative CommonMark fixtures
are accounted separately from the fixed Flark GFM extension profile so older
GFM-core expectations and unsupported footnote behavior cannot skew the score.
The current independent renderers differ in nine exact serializations across
the 189 pinned cases and in 225 incomplete typing/erasing revisions. This is
why peer consensus is diagnostic: pinned spec/profile expectations and explicit
adjudication decide behavior, while a candidate must always equal its own clean
parse at every revision.
