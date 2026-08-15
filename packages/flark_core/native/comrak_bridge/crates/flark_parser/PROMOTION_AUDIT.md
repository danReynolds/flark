# M1.1 exact-controller production-promotion audit

Status: production-shaped exact-clean slice and source-backed cooked-reference
publication implemented; M1.1 remains open.

## Selected donor boundary

The private parser crate now vendors the crates.io source for Comrak `0.54.0`,
upstream commit `172c2ee7d2c5c262a28be3e407aadf705daea2b7`, published checksum
`0d5910408554659ed848ff469e67ec83b30f179e72cec286cfdae64d1616f466`.
The substantive patch is one private `block_spine_facade` plus its module
wiring. The facade contains no full-parser oracle. Its production reference
value seam exposes only one exact entity decoder; atomic URL/title cleaners
exist solely as differential-test oracles.

This is **lexical-only consumption**, not a claim that Comrak itself has been
reduced to a stripped lexical API. The exact upstream source is vendored because
the generated scanners, table prefilter, and reference helpers are private and
their callable closure is broad. Production `flark_parser` imports only
`comrak::block_spine_facade`; an executable compile-boundary guard forbids
`parse_document`, `markdown_to_html`, and the research oracle in production
controller sources.

Two small facade helpers mirror donor control code that was not already a
generated scanner:

- thematic break: upstream `Parser::scan_thematic_break_inner`, lines
  1442-1477, SHA-256
  `b39e1d65357f6cbfe953baf9cb769ebff2158f08fe1eaa60bc2fdcc4cfe4a675`;
- list marker: upstream `parse_list_marker`, lines 2767-2882, SHA-256
  `25ea7a12c9bb73b50116acd4437c50e5dff78a8a21d2fafaf0075a95bc9e852b`.

`tests/promotion_audit.rs` locks both upstream functions and the corresponding
local helper bodies by line range and SHA-256. Any donor or mirror drift fails
closed and requires an explicit intake review.

The facade also exposes exact source cuts for a direct inline link/image tail.
It delegates destination and title recognition to Comrak's existing
`manual_scan_link_url`, `spacechars`, and `link_title` scanners; Flark retains
the returned source ranges and performs its own resumable source-backed value
cleaning. The promotion audit locks this local orchestration body as well.

## Controller boundary now implemented

`M11CleanBlockController` owns:

- one move-only begin/poll/commit/cancel admission per physical line;
- immutable source identity, exact ordinal/range progression, and EOF length
  validation;
- sequential source reads under both caller fuel and 4 KiB source grants;
- an outer parse job whose transition accounts at most one 4 KiB aggregate
  quantum across counted discovery bytes, explicit line admission/commit/EOF
  state, and lexical/classifier work rather than one transition per byte or
  parser phase;
- one bounded donor-opener stage per poll in normative order: block quote, ATX,
  fence, HTML, Setext, thematic break, list, indented code, then GFM Table;
- typed source-backed `Unknown` for every unsupported winning opener;
- no Table-candidate fallthrough to Paragraph;
- empty and one BOF-to-EOF root-Paragraph terminal results; and
- leading definition recognition with definitions-only and visible-remainder
  terminals, including the definitions-only Setext rule that retains the
  Paragraph rather than retrying `---` as a thematic break.

Every physical-line size now uses the same Flark-owned segmented lexical fold.
The fold retains at most a 128-byte prefix for finite-prefix HTML donor rules;
ATX, fence, Setext, thematic, list, HTML type 7, and Table-candidate facts use
constant streaming state. Opener precedence remains solely in the controller.
There is no regex, size branch, or separately supplied Paragraph
classification seam.

Leading reference definitions now use one forward-only DFA. It retains only
the CommonMark-bounded label and publishes exact source ranges for the
definition, label, destination, and optional title. Destination and title
payloads are never copied into parser state. Multiple definitions use a
one-byte `[` replay at their shared boundary; malformed title fallback records
the accepted prefix without replaying or materializing the suffix.

## Segmented lexical provenance and drift receipt

`segmented_lexical.rs` is Flark-owned because resumability, fuel, cancellation,
and source-range publication are product architecture. Its semantics are not
an independent Markdown grammar:

- block folds correspond to the pinned Comrak `scanners.re` rules plus the
  already locked thematic/list donor functions;
- destination recognition corresponds to Comrak
  `manual_scan_link_url{,_2}`;
- reference orchestration corresponds to the private facade's
  `reference_definitions`/`reference_definition`; and
- label normalization still calls the pinned facade directly.

`tests/promotion_audit.rs` locks those donor modules/functions by line range and
SHA-256. `segmented_lexical` then runs deterministic differential tests against
the pinned facade over 20,000 physical lines and 10,000 logical reference
prefixes. Upgrade drift therefore fails as either a source receipt or semantic
differential, rather than silently creating a second parser.

The exact-controller integration suite additionally proves 10 MiB special
lines, destinations, and titles under bounded retention and per-poll fuel, plus
cancellation after 17 unique bytes with no partial grammar publication.

## Source-backed cooked reference values

Accepted destination/title cuts now remain tied to the certified immutable
source lease through publication. One caller-fuelled `ReferenceCooker` performs
three bounded passes over each exact cut:

1. probe destination trim or title delimiters;
2. count the exact cooked length; and
3. replay the same selected source range while emitting directly into fixed
   arena blob pages.

The count pass lets the persistent reference root preflight exact page and byte
capacity before accepting any value bytes. Neither destination nor title is
flattened into a document-sized `String` or `Box<[u8]>`; the active state is one
4 KiB source window, one 32-byte entity/replay candidate, one 16-byte output
chunk, and one arena blob page. Candidate abort returns or drops the source
baton, clears bounded cleaner output, and transfers every partial arena owner
to fuelled reclamation.

Value semantics are pinned to Comrak 0.54's `clean_url`, `clean_title`,
backslash `unescape`, and exact generated entity table. Production calls the
facade only to decode one complete entity candidate. The promotion gate locks
those donor functions/modules and the 726-line research service receipt by
SHA-256, then differentially checks all 2,000+ semicolon entities, 10,000
random accepted values, Unicode and invalid-entity edges against the atomic
Comrak oracle.

The storage gate proves atomic and streamed multi-page facts have identical
canonical role metadata and queryable values. The parser publication gate
proves exact UTF-8/UTF-16 cuts through an independent host install, complete 10
MiB destination and 10 MiB title publication with writer fuel one and at most
three 4 KiB windows of conservatively reported retained value bytes, plus
cancellation both during a 10 MiB probe and after partial multi-page emission.

## Honest open boundary

The prior 8 KiB atomic lexical ceiling and `AtomicLexicalLimit` result are
removed. Line length no longer changes grammar behavior or forces aggregate
reference-prefix retention.

Endpoint fuel is also no longer sensitive to ordinary physical-line shape.
Fuel-1/fuel-32 equality and bounded transition envelopes cover giant lines,
80-byte lines, newline-dense text, CRLF/Unicode, empty input, and a CR exactly
at a discovery-window boundary. Mid-discovery and mid-lexical supersession
publish no partial candidate. Manual release receipts retain 1 MiB/10 MiB
giant, ordinary-line, and newline-dense timings; the five-million-line
adversary remains explicit rather than taking a narrow-grammar shortcut.

The source-backed GFM Table detector must still replace the current
conservative candidate Unknown.

The sibling `publication` boundary now turns exact-clean results into canonical
multi-page SourceFacts, Green, Projection, paged References, and CleanEof roles,
streams their self-contained closure through both credited production
endpoints, installs it in independent native/main-context-Wasm hosts, and
answers the same Rust-authored bounded public point query. A product-facing
resumable reference-query cursor, archive consumers, broader lifecycle stress,
the exact Table join, and final combined release gates remain; these focused
results are not a launch claim.
