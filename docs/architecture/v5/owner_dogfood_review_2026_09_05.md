# Owner-reported authoring gaps — 2026-09-05

**D0-web is reopened.** The owner found three common interactions missing from
the earlier readiness evidence. Vertical navigation and checkbox pointer
feedback are corrected in this candidate. Typed fence creation remains an open
authoring decision and B1; the parser's handling of unclosed imported Markdown
is correct. The [receipt](receipts/2026-09-05-owner/candidate.json) distinguishes
the two fixes from that open behavior.

## Diagnosis

| Report | Reproduction and owner | Status |
| --- | --- | --- |
| Up cannot traverse successive empty lines. | In `before\n\n\n\nafter`, Up at the end of `after` repeatedly stayed at source offset 15. The host sampled four pixels above the caret, landing in the current row's padding. The failure also affects ordinary row boundaries. | Fixed in the Flutter surface. Within a row, navigate using laid-out line metrics; at its edge, explicitly cross the row rectangle. Preserve the horizontal goal and selection base. |
| Starting a fence captures everything below. | Type three backticks on the empty line before `after`. The parser correctly recognizes an unclosed fence, hides its opening line and renders `after` as code. Return merely adds another code line. | Open authoring behavior. The build plan explicitly deferred fence auto-close to M6. That deferral was incompatible with claiming the ordinary writing loop ready. |
| Checkboxes have a text cursor. | The whole editor supplied a text cursor; checkbox activation had a separate hit test but no cursor annotation. | Fixed. The exact activation hit target now supplies the click cursor. The normal text cursor resumes outside it; read-only content does not advertise an action. |

The navigation correction was tested against empty lines inside a fenced code
block as well as between paragraphs. That neighbor exposed fractional line
heights: classifying a caret against the preceding line's bottom can choose the
wrong line. The implementation matches the actual line top instead. Tests
check both directions, every resulting painted caret, Shift extension, the
next typed character, Undo, and existing wrapped-line behavior. Checkbox tests
use Flutter's real mouse tracker and exercise hover, toggle, Undo, leaving the
target and switching to read-only while the pointer remains in place.

## Fence interaction to settle

The recommended contract is: type an opening fence, optionally with a language
tag, then Enter creates a bounded empty code block with the caret in its body.
Existing content below stays outside the block. The alternate interaction is
immediate empty-block creation on the third backtick. The owner was asked to
choose; this candidate does not silently implement either interaction.

Whichever trigger is selected, the implementation must establish the whole
authoring sequence before another handoff:

1. Type the opener character by character before existing prose, a heading,
   and an existing code block. Check the unaffected content on every paint,
   including before Enter; a correct final parse is insufficient.
2. Support an optional language tag and an empty body. Keep opening/closing
   syntax, the source caret and visible input target coherent.
3. Type code, insert blank lines, exit the region, then continue ordinary prose.
   Check source and block membership after every command.
4. Undo and Redo creation, body input and exit. Cover creation at EOF and in
   the middle of a document, and both backtick and tilde fences.
5. Preserve literal CommonMark behavior for imported/pasted Markdown and exact
   source mode. Recognition stays with Rust; an authoring command may create
   canonical paired source, but the parser validates the claimed structure.

This is a product rule requiring a complete sequence, not a parser exception
or an instruction to retain stale rendering below an unclosed fence.

## Methodology reflection

These are coverage and readiness gaps, not unusually hard edge cases. The
fractional-height detail is subtle, but moving through adjacent blank lines is
ordinary use and should have exposed the larger failure much earlier.

The prior tests were stronger at editing existing styled fixtures, checking
source invariants and demonstrating wrapped movement than at creating blocks
from typed input or moving between short/empty rows. Checkbox tests established
pixels and activation but did not establish discoverability. Fence creation
was deferred in the plan while broad authoring readiness language concealed
the missing behavior. More random histories would not repair those omissions.

The correction to the process is to test complete authoring sequences, starting
from ordinary typing and including the surrounding document. For each supported
structure: create it, edit it, leave it, delete it and Undo/Redo. For navigation,
cross short, empty, wrapped and differently styled rows in both directions.
For controls, check hover, activation and disabled/read-only feedback together.
Use explicit user-intent expectations for unaffected content and the next key.
Keep the cases direct; a new replay framework or a larger test-count target
would not address the gap.

The existing AppKit/performance and device gates remain open. This pass changes
only the Flutter surface and its tests; it does not requalify the kernel,
parser, native timing or other browser engines.

## Current verification

The full Flutter host suite passes **110 tests**, the workbench passes **25**,
and real Chrome transport passes **3**. Both analyzers and the normal
`flutter build web --wasm` pass. Navigation and hover regressions demonstrated
meaningful failures before the corrections. The previously qualified kernel
and parser inputs remain unchanged.

The rebuilt embedded browser (1073 × 899) was exercised with actual arrow keys,
typing, Undo and pointer clicks. Up and Down traversed each blank line; code
navigation reached every blank code line and accepted the following character.
The browser's actual cursor style was `pointer` on the checkbox and `text` on
its label. Toggling and Undo retained exact source. The disposable Draft was
cleared and the owner's existing Tour was restored without editing its content.
These successes do not close the fence-creation issue above.
