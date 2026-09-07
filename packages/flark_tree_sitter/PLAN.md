# Shared snippet language services

The public package is `flark_tree_sitter`. Code bodies have no dependency on Markdown
fence syntax, Flutter, or Fleury. Keep the Rust crate inside the package so the
package can be built without a sibling repository checkout.
See [ARCHITECTURE.md](ARCHITECTURE.md) for why the adapter uses Rust and the
conditions that would justify a Dart-only runtime.

## Ownership

- Rust: unmodified Tree-sitter and upstream language grammars, theme-free
  highlighting, syntax-aware indentation decisions, bounded snippet analysis.
- Dart: a synchronous language-service API after initialization, typed UTF-16
  ranges and proposed edits, native FFI and browser Wasm transports.
- Flark: Markdown fences/container prefixes, source and selection authority,
  input intent, and the single transaction that applies an accepted suggestion.
- Flutter/Fleury: input and rendering. No host-specific language rules.

Use the same Rust implementation and data on native and web. Browser creation
is asynchronous; editing calls are synchronous. A language query/grammar change
must not silently introduce a host-specific result. No JavaScript editor runtime
or second selection/history owner is introduced.

## Single-engine owner follow-up (2026-09-07)

The owner explicitly asked to remove the second maintained integration.
[MIGRATION_PLAN.md](MIGRATION_PLAN.md) and [SINGLE_ENGINE_REVIEW.md](SINGLE_ENGINE_REVIEW.md)
supersede the catalog boundary below: all 14 existing language choices now use
the shared engine. The larger twenty-language expansion remains separate.

## Earlier Ruby owner follow-up

Ruby was exposed by the fallback highlighter without a Tree-sitter edit profile.
The owner report advances this one language into the shared engine with its own
authoring corpus and host/browser checks; see [RUBY_REVIEW.md](RUBY_REVIEW.md).
This bounded repair does not close the existing performance gate or authorize
the remaining twenty-language expansion.

## Milestones

**Checkpoint:** milestone 1 and the functional portion of milestone 2 pass their
component gates. Milestone 2's combined typing-performance gate remains open.
The synchronous cost has been reduced and an optional coloring worker has a
component prototype. See [PERFORMANCE_REVIEW.md](PERFORMANCE_REVIEW.md) and
[RFC 031](../../docs/architecture/rfc/rfc_031_code_color_worker.md) for the
worker design. The Flutter workbench now implements milestone 3's host boundary;
see [HOST_INTEGRATION_REVIEW.md](HOST_INTEGRATION_REVIEW.md). Automated host checks
pass. The [browser dogfood review](BROWSER_DOGFOOD_REVIEW.md) completes the first
hands-on exploration and records three host fixes. The candidate is available
for exploratory owner feedback; sustained color-transition capture, full-frame
profiling and device qualification remain open before this milestone closes.

1. **Package and transport:** a runnable package with Dart, JavaScript, Python,
   and YAML grammars; exact source/range preservation; native/Wasm parity;
   explicit ownership, error, and disposal contracts. No production editor
   adoption based solely on component tests.
2. **Four-language authoring:** shared indentation semantics, normal Enter,
   typed closers, branch alignment, YAML scalars, tabs, CRLF, Unicode, literals,
   incomplete code, selection replacement and undo. Resolve the previous Python
   `else:` and YAML `settings: |` failures. Use upstream rules where compatible;
   retain provenance and reject unsupported rules instead of ignoring them.
   Qualify typing cost before expansion: the first full-snippet baseline at
   roughly 8K UTF-16 units costs 6–12 ms native and 8–16 ms under dart2js/Node
   before host work. Measure parse, query, encoding and transport costs; choose
   the smallest supported reuse/invalidation strategy that meets the combined
   budget. Source-size admission alone is not that proof.
   The functional corpus now has 85 cases, each asserting source, selection and
   the next typed character. The old Python branch and YAML scalar failures pass.
   Host undo and Markdown container mapping are verified in milestone 3 rather
   than simulated inside this stateless language package.
3. **Flark host adoption:** replace the current matcher only after the component
   contract passes; verify exact code-body mapping in nested Markdown, Flutter
   selection/input, following characters, and native/browser dogfooding. Measure
   cold load, per-keystroke cost, long lines and shipped asset size.
4. **Twenty-language qualification:** add TypeScript, Rust, Go, Java, C, C++,
   C#, Kotlin, Swift, Ruby, PHP, Bash, SQL, HTML, CSS and JSON. Each language has
   an explicit capability record and authoring corpus. JSX/TSX and mixed-language
   documents require named cases; catalog membership is not qualification.

Milestone 1 must establish a viable released runtime on both native and browser
Wasm before committing to its indentation query interpreter. Full-editor
formatting and language-server services are outside this snippet contract.

## Acceptance and review

Test real edit sequences, not just final colors: source, proposed edit, resulting
caret, next typed character and undo. Include literal contexts and incomplete
syntax. Native/Wasm identity is a component gate; Flutter input/paint, physical
devices and downstream Fleury integration remain separate evidence.

Review after the four-language milestone. Expansion should principally add
language data and tests. Repeated language-name branches in the engine, grammar
forks, unsupported upstream API access, or inability to meet the existing snippet
budget reopen the design decision rather than becoming accepted maintenance.
