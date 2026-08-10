# Flark v4 performance evidence contract, version 1

**Status:** M0 frozen. This contract defines what a performance receipt must
contain and how it is evaluated. It does not itself claim that Flark passes any
workload.

The normative workload matrix is
[`benchmark/v4/workloads_v1.json`](../../../../benchmark/v4/workloads_v1.json).
The normative receipt shape is
[`benchmark/v4/result_v1.schema.json`](../../../../benchmark/v4/result_v1.schema.json).
`result_v1.example.json` is synthetic schema data and is never measurement
evidence.

## 1. Workload identity and fixture generation

An executable workload ID is:

```text
flark-v4.<target>.<size-tier>.<shape>.<operation>
```

The matrix owns every permitted component. A receipt with an ID that cannot be
expanded from a checked-in matrix row is invalid. The fixed product sizes bind
to these exact UTF-8 byte counts: 1 KiB = 1,024; 25 KiB = 25,600; 100 KiB =
102,400; 1 MiB = 1,048,576; 2 MiB = 2,097,152; 5 MiB = 5,242,880; and 10 MiB =
10,485,760. Labels, decimal approximations, and runner-selected values cannot
substitute for those bytes.

The leading-editor boundary, its next meaningful tier, and the engine-only 4x
detector are competitor-derived. Each derived result must cite the frozen
repository-relative resolution path
`benchmark/v4/competitor_resolution_v1.json`, the SHA-256 of its exact checked-in
bytes, and that receipt's unique `receiptId`. The receipt must identify suite
`m0-mac-two-peer-suite-v1` and protocol `m0-mac-competitor-profile-v1`, and must
set `mayResolveCompetitorDerivedSizeTiers` true. A seven-field summary is not
authority. Resolution replays the full coordinator receipt: its plan file hash
and canonical hash
`3daf93557b1ac671b4c9a2aaa743276d8d629758999398073bd7da6b2b370d8c`,
exact 234-process and three-group denominators, plan/process-ID bijection, both
peer results, result/stdout/stderr hashes, and each nested raw-timeline and
final-export artifact edge. Completion must be eligible with no blockers. The
two peer tiers, cohort boundary, and next tier are recomputed from those 234
eligible results.

Resolution uses the public `PeerSuitePlan.fromJson`,
`PeerProcessEvidence.fromJson`, `RunGroupEvidence.fromJson`, and
`PeerSuiteValidator` implementation as its semantic authority. That replay
enforces the five-minute idle intervals, exact Latin-square/adjacent-peer
chronology, fresh-process profile configuration, workload input denominators,
strict accept-to-frame ordering, byte-exact exports, and paired paste/reset
state proofs. Every field copied from the resulting `PeerSuiteAssessment`—
eligibility, blockers, peer/cohort/next tiers, and validated-process count—must
match exactly. The default/check-in path uses `const PeerSuiteValidator()` and
the real frozen 1/5/10 MiB fixtures. Only unit tests may explicitly inject
`PeerSuiteValidator.testOnly(...)`; a receipt cannot select that authority.

The boundary is exactly `cohortCompletedTierBytes`. The next tier is
independently recomputed from the frozen 1 MiB -> 5 MiB -> 10 MiB -> 20 MiB
sequence and must also equal the receipt's `nextCompetitorTierBytes`. The engine
detector is exactly four times the boundary. This authority is macOS-only; it
cannot resolve Android or iOS tiers. Until an eligible full receipt and its
artifact graph are preserved, all three derived tiers are unresolved.

Fixture recipes operate on ASCII literals and resolve to an exact UTF-8 byte
count. The workload shape ID must equal the fixture recipe ID. Validation
regenerates the fixture deterministically from that checked-in recipe and
resolved target, then requires exact byte count and SHA-256 equality; runner
self-attestation is insufficient.

The complete product matrix includes ordinary prose, delimiter-dense text, a
giant paragraph, a giant physical line, many tiny blocks, nested containers,
GFM tables/task lists, many references, and an open fence to EOF. Its operations
cover cold open, warmed local edit, sustained typing and deletion, streaming
append, undo/redo, 32 KiB paste, non-local reference retargeting, and fence
close/reopen.

Every operation owns an exact state machine. The raw record retains each named
stage's revision, UTF-8 byte count, source hash, caret offset, edit offset, and
inserted/deleted text. Validation regenerates the initial fixture, replays every
warmup and measured operation, and chains each run's final state into the next
iteration. Insert, typing, deletion, 4,096-byte append, undo/redo intermediates,
reference replacement, and fence close/reopen therefore cannot be represented
by unrelated hashes or a no-op.

Every 32 KiB paste iteration proves `before -> pasted -> reset`, with the
inserted payload regenerated to exactly 32,768 bytes. It exports and hashes the
post-paste source, then resets and re-hashes the pre-iteration source; paste
cannot accumulate. Reference retarget alternates
`https://changed-a.invalid/` and `https://changed-b.invalid/` by operation
ordinal, including warmups. Every measured retarget must advance revision and
change both source and distant visible projection hashes.

## 2. Receipt provenance

Each receipt records:

- repository URL, exact commit, dirty state, and dirty-diff hash when relevant;
- the workload and schema file paths and hashes;
- exact argv arrays, working directories, and performance-affecting environment;
- fixture generator/recipe, resolved size, actual size, and source hash;
- named hardware, CPU, logical cores, physical memory, device class, OS, and
  architecture;
- Dart, Flutter, Rust, Cargo, Xcode, and Android toolchain identifiers;
- engine and Flutter revisions, build mode, and application artifact path,
  byte length, and hash;
- physical-device/simulator state and actual display refresh rate;
- operation ID and iteration unit; exact warmups per run, samples per run, run
  count, cadence, total sample denominator, visible-character count, and hashed
  raw traces;
- a target-bound measurement surface: `flutter-product` for product workloads
  and `engine-only` for the 4x hidden-linear-work detector;
- fixed or competitor-receipt size-resolution kind, resolved bytes, and (for a
  derived tier) the checked-in receipt path, hash, and unique receipt ID.

The receipt's resolved threshold object is derived, never chosen by the runner.
It must match the named frozen profile exactly after evaluating its two formulas
from the recorded fixture byte count, visible-character count, and display frame
period. Tier A is valid only on macOS. Tier B is valid only on Android or iOS;
its physical-device and simulator rules still apply independently.

Real claim-eligible receipts must use a clean commit and a profile-mode product
artifact for product workloads. Every build and raw artifact must exist and
match its declared byte length and SHA-256. The synthetic example deliberately
points to no evidence, sets `receiptKind: schema_example`, and sets
`claimEligible` false; it can never be promoted by changing one Boolean.

The frozen render denominator is 600 x 600 logical pixels at DPR 2 and text
scale 1, using bundled `FlarkBenchmarkMono-v1` at 16 logical pixels, 1.25 line
height, weight 400, and zero letter spacing. At least 512 characters must be
visible. A product receipt records those values exactly. Raw render evidence
then binds the painted source revision/hash to its visible UTF-8 range and
content, visible-text hash, glyph count, deterministic glyph-run hash,
projection hash, raster hash, proving frame, and raster-finish timestamp. The
receipt's visible-character count is the minimum derived range length across
those proofs, not a runner-copied scalar. This prevents a runner from winning by
shrinking text, viewport work, or visible content.

The engine 4x target is intentionally engine-only. Its render surface, latency,
frame metrics, Flutter build/layout/paint/raster distributions, and uncertified
visible-character metric are `null`/not applicable. It retains engine, FFI,
convergence, memory, and lifecycle evidence. A Flutter-hosted 4x experiment
would require a separately versioned target rather than silently applying or
claiming product gates.

## 3. Frozen sampling, distributions, and outlier rules

Every operation uses the same normalized sampling vocabulary. Warmups occur in
each run and are excluded from distributions. The total denominator is exactly
`sampleIterationsPerRun * runCount`:

| Operation | Unit | Warmups/run | Samples/run | Runs | Cadence | Total |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| cold-open | fresh process open | 0 | 1 | 30 | 0 Hz | 30 |
| warmed-local-insert | edit | 20 | 200 | 3 | 0 Hz | 600 |
| sustained-typing | edit | 20 | 600 | 3 | 60 Hz | 1,800 |
| sustained-deletion | edit | 20 | 200 | 3 | 60 Hz | 600 |
| streaming-append | append | 4 | 128 | 3 | 0 Hz | 384 |
| undo-redo | cycle | 5 | 100 | 3 | 0 Hz | 300 |
| paste-32kib | paste | 2 | 30 | 3 | 0 Hz | 90 |
| reference-retarget | edit | 10 | 100 | 3 | 0 Hz | 300 |
| fence-close-reopen | cycle | 5 | 100 | 3 | 0 Hz | 300 |

Cold open therefore means 30 fresh processes, never 30 opens in a warmed
process. A receipt must copy its operation's iteration unit, warmup, sample,
run, cadence, and total values exactly. Every distribution in that receipt must
have `sampleCount` equal to its declared denominator: operation/latency/
convergence distributions use the frozen measured-sample total, while frame
and per-frame FFI distributions use the complete retained frame-stream count.
A runner cannot preserve a `PASS` by reducing either denominator or silently
dropping failed observations.

Claim evidence contains one `flark-v4-raw-evidence-v1` artifact, bound to the
exact workload-matrix and result-schema hashes in its receipt. Every warmup has
a unique run/index/process identity, ordered process-local timestamps, and the
same operation-state proof as a measured iteration; warmup counts must exactly
match every run. Every sample
has unique run/sample/process/frame IDs plus scheduled, accepted, source-paint,
caret-paint, and selection-paint timestamps. It also records source revisions
and hashes, distant-projection hashes, phase timestamps, synchronous spans,
work-unit and pump IDs, convergence, and operation-specific reset/retarget
proof. Each sample declares its inclusive start/end vsync ordinals and one
proving frame. Frame records retain run/process identity, contiguous vsync
ordinal, attribution, linked sample/work/pump IDs, exact phases, and individual
FFI calls. Memory records are timestamped per-process baseline, peak, close, and
post-close points. Lifecycle evidence uses process-bound open/edit/close cycles,
reopens, background/foreground cycles, sustained intervals, thermal samples,
events, and final live-state samples.

The validator replays raw evidence instead of trusting aggregates:

- nearest-rank p50/p90/p99 and maxima are recomputed from exact raw samples;
- phase duration is finish minus start, and synchronous maximum is the longest
  recorded span;
- every vsync ordinal in every measurement interval must exist exactly once;
  extra and omitted frames are invalid, including non-proving frames;
- frame build/raster/editor work, total span, misses, editor-attributed misses,
  miss rate, and hard maximum are recomputed from the complete frame stream;
- FFI totals and per-frame distributions are recomputed from individual calls;
- allocation/RSS is recomputed only after proving each process's ordered
  baseline/peak/close/post-close points; lifecycle IDs and timestamps must name
  real processes, remain inside their process intervals, and prove close,
  reopen, and post-close/final-live-state ordering;
- exact per-run indices, 60 Hz scheduled timestamps within one microsecond, and
  30 distinct cold-open processes are enforced.

Any published count, percentile, maximum, rate, cadence, memory, or lifecycle
value that differs from replay invalidates the receipt.

Every timed or per-frame distribution records `sampleCount`, p50, p90, p99, and
maximum in the declared unit. Values must be monotonic:

```text
p50 <= p90 <= p99 <= maximum
```

A percentile never hides the maximum. Receipts separately record the longest
synchronous span and every editor-attributed missed frame. Frame misses are
evaluated against the actual recorded display period, including displays faster
than 60 Hz.

Display provenance must satisfy
`displayFramePeriodMicros == 1,000,000 / displayRefreshHz` within one
microsecond. For every raw sample, source, caret, selection, and their latest
paint must occur no later than `min(16,000 microseconds, actual frame period)`
after acceptance. The selected proving frame must begin building strictly after
acceptance, belong to the same process, and finish raster at the latest paint
timestamp. Its work-unit and pump IDs must match the sample. Non-proving frames
remain in the denominator and can independently create a miss.

Required metrics separately cover accepted source, caret, and selection
visibility in frames and microseconds. For each input sample,
`inputToPaintMicros` ends at the latest of those three visible events; its p50,
p90, p99, and maximum therefore cannot be earlier than the corresponding value
for any constituent distribution. Metrics also cover cold exact viewport paint,
visible certification, foreground work by layer, build/layout/paint/raster work,
frame totals and misses, FFI calls and returned bytes, convergence work/pumps/
wall time, uncertified visible character-frames, allocations, RSS, and repeated
lifecycle state.

Cold exact viewport paint is applicable only to `cold-open`; it is `null` for
every other operation. Operation-specific raw requirements for paste and
reference retarget are mandatory only for those operations.

## 4. Tier A Mac gates

For `tier-a-mac-m0-v1`:

- accepted source, caret, and selection each have an explicit one-rendered-frame
  gate and the raw microsecond gate above; the accepted state is not visible
  until the latest of all three;
- no input backlog is older than one frame;
- synchronous Rust engine work is at most 4,000 microseconds at p99;
- profiled Flutter frame work is at most 8,000 microseconds at p99;
- every editor-attributed frame and synchronous span is strictly below 16,000
  microseconds;
- editor-attributed dropped frames are zero;
- first exact editable viewport paint is strictly below 200,000 microseconds;
- visible current-revision certification and convergence are strictly below
  500,000 microseconds;
- uncertified visible character-frames are bounded by the visible character
  count times the number of display frames in 500 milliseconds;
- peak RSS over the warmed baseline is at most the greater of 64 MiB and eight
  times source bytes; retained RSS over baseline after close is at most 16 MiB;
- after at least 100 open/edit/close cycles and 10 process reopens, live
  documents, transactions, continuations, and handles are exactly zero.

The p99 gates are inclusive. Thresholds described as “below” or “never reaches”
are exclusive. A specific typed terminal fault satisfies the no-silent-stop
protocol but makes the workload result `FAIL`; it is never counted as
convergence.

## 5. Provisional Tier B mobile gates

`tier-b-mobile-provisional-m0-v1` predeclares the same interaction, p99, hard
frame/span, cold-paint, certification, convergence, and zero-drop product gates.
It is a target, not a mobile claim. Only a named physical Android or iOS device
can produce a claim-eligible Tier B receipt; simulators are always ineligible.

The provisional mobile memory guard is the greater of 48 MiB and six times
source bytes over baseline, with at most 8 MiB retained over baseline after
close. The run also requires at least 100 open/edit/close cycles, 10 process
reopens, 20 background/foreground cycles, 30 minutes sustained execution, and
zero observed thermal-throttle events. The named device's competitor envelope
must be resolved from evidence on that same platform before its scale run. The
frozen Mac peer receipt is forbidden as mobile size authority. Android and iOS
produce separate receipts; one cannot qualify the other.

Changing any threshold requires a versioned contract amendment and a fresh run.
It cannot retroactively turn a failed receipt green.

## 6. PASS and FAIL

`PASS` means every applicable predeclared gate passed and all required raw
artifacts are present. Missing provenance, reduced or mismatched sampling, a
distribution with the wrong total denominator, a dirty claim build, an
arbitrary fixed size, a missing/mismatched/ineligible derived-size receipt,
incorrect canonical peer plan/artifact graph/next-tier/4x arithmetic, a fixture
regeneration mismatch, missing or hash-mismatched raw evidence, aggregate/raw
disagreement, a receipt/raw contract-hash mismatch, missing or forged warmup,
operation no-op or broken cross-sample chain, incomplete frame ordinal stream,
incoherent display timing, delayed source/caret/selection, cadence drift, reused
cold process, accumulating or non-32-KiB paste, stale reference retarget,
unreplayable visible range/glyph/projection/raster proof, unbound or reordered
memory/lifecycle evidence, wrong render/target applicability, non-monotonic
distribution, stale or silent terminal state, a single hard-span violation, one
editor-attributed dropped frame, or non-zero live state after close makes the
receipt invalid or failed as appropriate.

Raw traces are evidence; this Markdown file or a narrative result is not a
receipt.
