# Comrak-correspondent value block core

Disposable decision gate for an exact Flark-owned block spine.

The implementation must preserve the selected Comrak 0.54 block function
ordering while replacing arena nodes and owned AST content with value state,
coverage-leaf-relative origins, and direct events. Stock Comrak's block parser
is forbidden on the candidate path. The pinned narrow lexical/table/reference
facade and bounded inline service are allowed.

Clean parsing is exact across Gate A and the full 1,322-fixture CommonMark/GFM
corpus. The same grammar also passes the serialized-continuation gate: after
every physical line, canonical open-frame state is JSON round-tripped, parser
scratch is discarded, and output is reconstructed through a write-only sink
with exact blocks, references, origins, and HTML.

Live scratch is sealed/rebuilt each physical line without requiring a persisted
JSON pause. A 2,000-line schedule with one persisted pause stays at four open
frames and seven transient nodes. The live rebuild consumes/moves open leaf
state and output receives append/drain/finalization deltas: a continuously open
1 MiB paragraph reports zero pending-prefix bytes copied and exactly 1 MiB of
logical delta payload. Persisted JSON pause remains a copy-only hidden-state
falsifier.

Raw fenced-code and HTML leaves now take a different ownership path. Their
`BlockKind` stores only constant-size logical projections; coverage-relative
origin runs point at immutable source leaves, finalization folds only scalar
range metadata, and append events copy zero aggregate literal bytes. Direct,
live, and streamed-render checks are donor-exact for multiline 1 MiB and
10 MiB fence/HTML inputs. Fenced info normalization remains Comrak's exact
entity/trim/backslash transform, but its 8 KiB input cap is checked before the
only intermediate string is allocated.

This is not yet a full production resource pass. The eager oracle still scans
16,385 descendants when the 1 MiB list closes, but the production-shaped lazy
overlay matches it over all 1,322 fixtures while recording one repair scope,
touching zero repair descendants, and performing zero final list scans. A
32-node visible position query remains bounded to those 32 nodes. Detached
sibling and 100-level nested-list adversaries forced the aggregate from
per-list multisets to a global point-update/range-maximum index; their maximum
point reads are now 10 and 201 steps respectively. The index is still backed
by resizing test vectors, not persistent output pages.

One-time child folds and iterative output traversal remove the suspected deep
close/finalize quadratic. A cooperative gate now drives the same parser through
explicit line and finish phase cursors: all 1,322 fixtures remain exact, no
poll copies the open frame stack, and a release 20,000-quote run stays at 256
transitions/events/index operations with maximum observed line/finish polls of
224/132 us. A continued-then-deindented 20,000-quote path remains below 235 us
per poll. This proves the parser loop can yield; it does not yet make the
existing continuation wrapper's whole-path materialization/compaction or sink
updates cooperative. Production therefore still needs persistent pages/leases,
edit convergence, event-page integration of the cooperative loop, paragraph
output/reference-definition aggregate ownership, generated refillable scanners
for oversized paragraph lines, and coalescing of the prototype's one source
`String`/origin run per physical raw-block line. Paragraph grammar equality is
already scalar: setext-visible content plus GFM last-line eligibility/count;
payload, preface, and reference cursors stay in the output root.

The control-authority cut is now complete. `ValueBlockParser` owns one typed
`LineTransition`/`FinishTransition` phase machine; the ordinary parser is an
unlimited-step caller and `FuelledValueBlockParser` contributes scheduling,
cancellation, and event-delivery budgets only. The former duplicate
match/open/text/finish loops were deleted after the complete fixture and
checkpoint suites remained green. See
[CONTROL_AUTHORITY_VERDICT.md](CONTROL_AUTHORITY_VERDICT.md) for the executable
receipt and the still-open output-production and retirement boundaries.

The oversized physical-line investigation now has a donor-local, source-backed
feasibility slice. `RevisionAuthority::lease_refillable_line` admits only the
existing private source identity plus certified scalar metrics in O(1), while
`RefillableLineJob` pulls at most a configured window (4 KiB in the giant-line
gate), carries UTF-8 across arbitrary refill boundaries, and separately meters
block-prefix recognition and body coverage. Ten focused tests include 10 MiB
paragraph and fenced-literal lines, one-byte UTF-8/fence-marker refills,
cancellation, wrong-source/metric rejection, and 512 randomized admitted lines
whose provenance cuts match `DirectValueBlockParser`. That differential found
and corrected an important ownership detail: leading paragraph indentation is
a Document-owned `Gap`, not Paragraph content.

This is deliberately not described as removal of the direct parser's 8 KiB
ceiling. `DirectValueBlockParser::begin_line` still owns a whole `String`, and
its parser-owned `LineTransition` still receives a whole `&str`; an executable
test requires that path to keep returning `LineTooLarge` even while the
standalone refillable proof succeeds. ATX headings, block quotes, list/thematic
markers, HTML starts, reference definitions, and setext candidates also return
explicitly to the donor grammar rather than being approximated here. The open
production seam is one trusted transition hook that accepts certified
recognition facts/ranges and streamed line metrics, then performs the existing
grammar mutation without rescanning source or creating a second parser.

`provenance/comrak_0_54_block_functions.json` is the machine-readable map from
all 55 selected upstream functions to their local correspondents. Regenerate
or check it with:

```sh
python3 scripts/generate_provenance.py --check
```

Reproduce the complete current gate with:

```sh
cargo test --all-targets
```

Run the aggregate raw-literal ownership/cancellation gate alone with
`cargo test --release --test literal_runs`.

See [RESULTS.md](RESULTS.md) for the gate ledger and remaining stop conditions.
The physical-line convergence field audit and its variant-local candidate key
are recorded in [TRANSITION_STATE_GATE.md](TRANSITION_STATE_GATE.md).
