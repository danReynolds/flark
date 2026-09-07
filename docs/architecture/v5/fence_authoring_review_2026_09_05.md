# Immediate fence authoring follow-up

The owner clarified that the third typed backtick should immediately open an
empty one-line code block. This supersedes the pending timing decision in the
[owner review](owner_dogfood_review_2026_09_05.md). The kernel now pairs that
opener before publishing, places the caret in the empty body, and leaves a
writable gap before existing following content. Tildes use the same rule.

## Implementation and review

The typing command asks Rust to recognize the candidate opener. The kernel
synthesizes its closing fence, then parses and admits the completed candidate
before recording history or notifying the host. Ordinary typing still parses
once; a recognized completion parses twice within the existing source bounds.
There is no provisional swallowed-tail frame, deferred repair, host delimiter
recognizer, or additional editor state. Quotes, list nesting, tab padding,
opening-line whitespace and CRLF have direct cases.

Completion is a separate Undo action. Enter continues the code body, including
empty lines inside containers. Down reaches the following writable gap.
Backspace removes an empty closed fence and keeps the container available for
prose. Paste, source mode, range replacement and IME preedit remain literal.
Language-tagged source is preserved; immediate completion sends subsequently
typed text into the code body. Bodyless imported fence syntax still requires
source editing for Return; it rejects atomically instead of indexing a hidden
line. This is distinct from newly authored fences, which always contain a body.

The full discovery run found an additional multiline inline-code boundary bug.
A shadowed Rust end variable corrected content but retained the wrong full
owner end, overlapping following text. Removing that shadow aligns both ranges.
The minimized case also exposed a missing rendered space at the inline code's
line ending. Projection now emits that parser-owned boundary as a styled space.
Both have direct regressions; the Rust range regression failed before its fix.

## Testing reflection

The original owner reports exposed a methodology gap. Consecutive blank-line
navigation, pointer feedback and creating a block above existing content are
ordinary authoring actions. The prior suite proved substantial editing behavior
on existing fixtures while the fence-creation feature remained deferred.
That did not justify a broad authoring-ready claim.

The correction is concrete intent-based sequences with surrounding content:
create, type, leave, delete, Undo and Redo, checking the next character and each
actual paint. The new mounted tests run that sequence through Flutter's text
input at 240 and 800 logical-pixel widths. The generated run complements those
examples: seed 2028 found a less obvious inline-code extraction defect, which was
reduced to a named scenario rather than preserved as a replay fixture.

## Evidence

Final local suites: **388 kernel, 113 Flutter host, 25 workbench and 3 real
Chrome transport tests** (529 Dart/Flutter/browser tests), plus **42 Rust tests**.
The kernel run includes 1,000 generated histories with seed 2028. All analyzers
pass. Native, bundled Wasm and freshly built Wasm agree on all 1,322 corpus
cases. The normal `flutter build web --wasm` is rebuilt for handoff.

Browser verification and final candidate hashes are recorded in
[the candidate receipt](receipts/2026-09-05-fence/candidate.json).
In the normal rebuilt embedded browser (1073 × 899), three separate typed
backticks created one visible empty code line above an existing heading and
language-tagged code block. Typing, Down to prose, Undo/Redo and empty Backspace
all retained exact source and carets. A quoted list fence retained its context
through Enter and typing. An empty document also completed correctly. Final
canaries traversed all four blank-line Up targets, showed the checkbox pointer
and a single toggle, and painted a normalized multiline inline-code space.
The disposable Draft was cleared and the owner's existing Tour was restored.
This remains a local dirty candidate on `v5/m2-kernel`, not named-commit CI,
native/AppKit qualification, a timing claim, or M5 platform certification.
