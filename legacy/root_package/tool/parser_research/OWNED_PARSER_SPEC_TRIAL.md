# Spec-first owned parser trial

Status: active staged trial, corrected 2026-07-14. This inverted the
parser-choice sequence in RFC 023: establish whether a small Flark-owned parser
can grow cleanly against pinned standards before spending more effort
productionizing the Comrak integration. The extracted Comrak fork remains a
development oracle, not a runtime dependency of this trial.

The original stop decision was based on incomplete/asymmetric evidence and is
superseded. Later probes selected a Flark-owned, Comrak-derived persistent core
as the funded direction, subject to the exact unified block/inline gates. The
current clean-room trial remains architecture evidence, not the production
grammar seed. See
[`PARSER_AUTHORITY_REASSESSMENT.md`](PARSER_AUTHORITY_REASSESSMENT.md) for the
current decision and [`OWNED_PARSER_TRIAL_RESULTS.md`](OWNED_PARSER_TRIAL_RESULTS.md)
for the evolving receipts.

## Original trial decision

There is a sufficiently clear standard against which to implement an owned
parser, with one important qualification: CommonMark and GFM specify Markdown
semantics, not a live editor protocol.

CommonMark 0.31.2 provides:

- declarative syntax and precedence rules;
- 652 Markdown-to-HTML conformance examples;
- an explicit two-phase parsing strategy (blocks, then independent inline
  leaves after reference definitions are known);
- detailed algorithms for the difficult delimiter/bracket work;
- a BSD-licensed reference implementation and test runner.

GFM 0.29 provides the five extensions Flark enables: tables, strikethrough,
bare autolinks, tag filtering, and task-list items. Its main spec has 670
enabled examples, including 22 extension examples. Its separate extension
corpus has 30 examples, of which 27 exercise Flark's enabled profile and three
exercise excluded footnote behavior.

The standards do **not** define exact source ranges, an AST schema, marker and
replacement ranges, stable identities, incremental checkpoints, cancellation,
binary deltas, or live presentation while work is pending. Those are not
reasons to retain Comrak; they are the small, explicit Flark parser-service
contract that must sit above either grammar implementation.

The recommended next step is therefore a standalone, parser-library-free Rust
crate implementing a bounded vertical slice. It should emit Flark-native
persistent chunks directly and be judged after roughly six to nine focused
engineering weeks. It must not be used in the product until the complete
profile passes.

## Normative profile

The trial should pin the following inputs. Machine-readable hashes and commit
IDs are in `OWNED_PARSER_SPEC_PROFILE.json`.

| Concern | Authority | Rule |
| --- | --- | --- |
| Core syntax | CommonMark 0.31.2, tag `0.31.2` | Normative grammar and 652 examples |
| Extensions | GFM 0.29 extension sections at pinned cmark-gfm commit | Only table, strikethrough, autolink, tagfilter, and tasklist |
| Core/extension conflicts | Flark profile | CommonMark 0.31.2 core wins; GFM contributes extension behavior only |
| Character entities | WHATWG HTML entity set, through a pinned generated table/crate | Data dependency, not parser authority |
| Product behavior | Flark migration corpus | Projection, edit intent, accessibility, and rendering policy |
| Incremental behavior | Flark parser-service contract | Every converged result equals a clean parse of the same revision |

The conflict rule is necessary rather than theoretical. The official GFM spec
is based on an older CommonMark core. Nine emphasis inputs shared with
CommonMark 0.31.2 currently have different expected HTML. A single modern core
with GFM extensions is more coherent than switching core emphasis semantics
when the GFM profile is enabled.

Call the resulting target **Flark CommonMark/GFM Profile 1**, not unqualified
“GFM 0.29.” Its profile identifier and pinned inputs must be present in every
snapshot and delta.

### Flark-owned clauses

The supplementary editor contract should be short and explicit:

1. Parser input is well-formed UTF-8 with LF line endings after Flark's public
   ingress normalization. Committed edits are not normalized by the parser.
2. All source coordinates are half-open UTF-8 byte ranges on scalar
   boundaries. UTF-16 conversion remains in the source substrate.
3. Semantic nodes and marker/replacement facts have independently specified
   ranges. A total source-coverage index includes syntax, whitespace, and gaps;
   the semantic tree is not the editable tree.
4. Raw HTML is classified according to CommonMark/GFM, but Flark's product
   renderer continues to show it literally and never executes it. Sanitization
   is a rendering/security concern, not a grammar shortcut.
5. Budget exhaustion produces `pending`, never a different parse. Resource
   limits, cancellation, and revision supersession cannot change semantics.
6. A converged incremental snapshot must be byte-for-byte equivalent to a
   clean parse under the same profile, including reference winners and source
   facts.
7. Unsupported rules are allowed only in the research trial. They are listed
   as not implemented, never counted as passing because their source fell back
   to literal text.

## Conformance system

The owned parser needs six cumulative lanes from its first commit:

1. **Normative semantics.** A test-only normalized HTML renderer runs the
   official CommonMark and selected GFM examples. Production code does not
   need an HTML renderer.
2. **Declared trial coverage.** A manifest records exact spec sections and
   example IDs supported by each milestone. The runner reports supported,
   failed, and not-yet-implemented separately.
3. **Structural/source facts.** Flark-owned goldens specify node kinds,
   ancestry, half-open byte ranges, markers, replacements, and total coverage.
   CommonMark's HTML examples cannot certify these.
4. **Incremental equivalence.** After every insertion, deletion, replacement,
   paste, and composition-shaped batch, compare the incremental snapshot with
   the candidate's clean parse. Periodically compare semantic output with
   cmark-gfm/Comrak as a separate oracle.
5. **Pathological and fuzz behavior.** Reuse cmark-gfm's nested delimiter,
   bracket, quote, list, fence, HTML, reference-collision, and table cases. Add
   arbitrary UTF-8 edit sequences, cancellation points, and allocation/work
   ceilings.
6. **Product migration.** Feed the parser facts through Flark's existing
   projection, command, input, widget, and accessibility corpus. A semantic
   conformance pass is necessary but not sufficient for cutover.

The current repository already contains all 652 official CommonMark examples
unchanged. Its copied 670-case GFM fixture is not an adequate pin for the new
lane: compared with cmark-gfm commit `499789b`, nine emphasis expectations are
stale, and the existing v2 upstream contract validates ranges/projection/
determinism without comparing the fixture's expected HTML. The owned lane
should vendor freshly generated, attributed fixtures under a new path rather
than silently changing the v2 migration suite.

Task-list examples also need an explicit supplement. Two examples in the GFM
main spec are marked disabled by its generic HTML runner, while the separate
extension corpus contains three executable task-list examples.

## Trial implementation

Create a standalone crate, initially under parser research, with no Comrak,
cmark, Pulldown, markdown-rs, or Tree-sitter production dependency. Reasonable
data-only/helper dependencies include a pinned HTML entity table and Unicode
default-case-fold implementation. Oracle adapters are dev-only processes.

The core should be small and shaped like the desired final system:

- `Input`: bounded reads over the existing mirrored UTF-8 rope;
- `BlockState`: a value-semantic open-container stack, pending leaf state, and
  extension state;
- `Checkpoint`: compact state plus source/context fingerprints;
- `BlockMachine::advance(budget)`: line-bounded work and exact convergence;
- `InlineMachine::advance(budget)`: explicit delimiter/bracket arrays with
  mid-leaf cancellation and continuation;
- persistent source-relative chunks and a first-definition-wins symbol index;
- direct marker, replacement, semantic-target, and coverage facts;
- the existing revision/hash-scoped binary delta boundary.

There should be no general mutable HTML AST. The conformance renderer consumes
the same immutable chunks as the editor adapter.

### Milestone 0 — foundation

Implement six complete CommonMark sections, 61 examples total:

- blank lines (1);
- paragraphs (8);
- textual content (3);
- soft line breaks (2);
- ATX headings (18);
- fenced code blocks (29).

This milestone must already support direct persistent chunks, exact byte
ranges, a test renderer, checkpoints inside a million-byte unclosed fence,
revisioned edits, convergence, cancellation, and clean-parse equivalence. It
is expected to take roughly one to two focused weeks after the harness exists.

### Milestone 1 — architecture stress slice

Add a deliberately limited, manifest-pinned set of normative examples that
exercise every hard architecture mechanism without claiming whole-section
conformance:

- nested block quotes and lists, lazy continuation, tightness propagation;
- indented and setext interactions that reclassify pending paragraphs;
- delimiter runs for emphasis/strong and strikethrough;
- code spans and escapes;
- inline and reference links, duplicate first-definition-wins behavior, and a
  missing definition that later becomes present;
- a table with escaped pipes and inline syntax;
- nested task-list items;
- an HTML block whose closing boundary lies far from the edit.

Each selected example ID and every extra Flark case belongs in the manifest.
Edits must occur at opener, closer, interior, and ambiguity boundaries, not
only in plain content.

This is the stop/go architecture gate. A focused implementation is expected to
take another four to six weeks; exact timing is a planning range, not a
commitment.

### Architecture pass criteria

Proceed toward full conformance only if the stress slice demonstrates:

- one explicit parser state model, without a shadow tree used only for resume;
- bounded checkpoints inside containers and inline leaves;
- cancellation/supersession at line, delimiter, and output boundaries;
- direct persistent chunks and deltas, without constructing and copying a
  second arena AST;
- semantic results independent of budget size and edit history;
- no unexplained oracle difference for declared rules;
- memory/checkpoint density compatible with the 10 MB SLA;
- a core that is materially easier to understand and test than the equivalent
  Comrak adapter path.

Failure means stop the owned lane and use the extracted Comrak fallback. A
failure is useful evidence, not a partial parser to ship.

### Expansion after a pass

Expand cumulatively by dependency, completing whole spec sections before
calling them supported:

1. all container and leaf blocks, list tightness, HTML blocks, and reference
   definitions;
2. escapes, entities, code spans, line breaks, emphasis/strong;
3. links, images, autolinks, raw HTML, and complete reference semantics;
4. the five GFM extensions and their interactions;
5. all 652 CommonMark cases, the pinned GFM extension corpus, Flark's
   structural/range contract, every-revision fuzzing, and native/WASM parity.

No product cutover occurs before the complete enabled profile and Flark's
behavioral corpus pass. Comrak/cmark-gfm may remain CI oracles until cutover,
but are never a simultaneous runtime grammar.

## Effort and decision value

| Work | Planning range for one experienced Rust owner |
| --- | ---: |
| Pin/profile, fixture generator, conformance renderer, manifest | 3–5 days |
| Milestone 0 | 1–2 weeks |
| Milestone 1 architecture stress slice | 4–6 weeks |
| **Owned-architecture stop/go evidence** | **about 6–9 weeks** |
| Complete production candidate if the gate passes | **still approximately 4–8 months total** |

The six-to-nine-week trial buys a much better decision than another synthetic
kernel: it forces the proposed architecture through persistent containers,
inline delimiters, global references, extensions, source facts, cancellation,
and local deltas while the code is still cheap to discard.

## Licensing and provenance

- CommonMark and GFM spec text/examples are CC-BY-SA 4.0; vendored test data
  needs attribution and license notices.
- Their test runners and cmark/cmark-gfm implementation code are BSD licensed.
- Lezer Markdown is MIT licensed.
- Any adapted algorithm receives a module-level provenance entry and the
  repository keeps a third-party notice inventory.

This plan is not legal advice, but it makes provenance a design input rather
than cleanup after implementation.

## Primary sources

- <https://spec.commonmark.org/0.31.2/>
- <https://github.com/commonmark/commonmark-spec/tree/0.31.2>
- <https://github.github.com/gfm/>
- <https://github.com/github/cmark-gfm>
- <https://html.spec.whatwg.org/entities.json>
