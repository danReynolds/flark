# M0 — certification probe: the engine is safe, and too coarse

**2026-08-06.** Probe: `example/lib/m0_certification_probe.dart`.

RFC 024 §4.4 requires semantic formatting only where certified for the *current*
revision. The counterexample that forced the rule was `*hello*` → delete the
closing `*`: if the engine keeps serving the old emphasis facts, a caller paints
emphasis over text the parser is about to call literal.

The probe applies an invalidating edit and asks — **before convergence** — what
the engine reports for the edited region and for a distant untouched region.

## Result

```
cert emphasis-opener-orphaned  before=RECURSIVE-GREEN(rev=1) | structureCurrent=false edited=PENDING distant=PENDING
cert strong-demoted            before=RECURSIVE-GREEN(rev=1) | structureCurrent=false edited=PENDING distant=PENDING
cert code-span-orphaned        before=RECURSIVE-GREEN(rev=1) | structureCurrent=false edited=PENDING distant=PENDING
cert setext-broken             before=RECURSIVE-GREEN(rev=1) | structureCurrent=false edited=PENDING distant=PENDING
```

## What this means

**The safety property already holds, and it holds structurally.** The engine
never serves stale semantics — it returns `PENDING` rather than old certified
facts. A caller *cannot* accidentally paint the v2 failure, because the engine
will not hand it the material. RFC 024 §4.4's correctness requirement is
satisfied by construction, not by caller discipline. That is the better of the
two possible outcomes and it is worth stating plainly.

**But certification is whole-document, and that is a real problem.** The distant
paragraph — genuinely untouched, and provably so — also goes `PENDING`. So *one
keystroke invalidates the entire document.*

Rendered naively per §4.4, that means every visible block loses its semantic
presentation on **every keystroke**, returning 1–2 frames later. Document-wide
flicker at typing speed. Correct, and unusable.

So the model in RFC 024 §4.4 — proven-reusable / invalidated /
dependencies-unevaluated / newly-certified — is **not** what the engine
implements. The engine implements two states, document-wide: current, or
pending.

## The M0 work this defines

**Expose per-range certification through the query API.** The information
already exists: an incremental parser knows exactly which blocks it reused. The
query layer simply does not surface it — it answers "is the whole structure
current" rather than "is this range certified for this revision".

That is plumbing, not new theory, and it is squarely M0 because every rendering
decision above it depends on the answer.

## A new argument for incrementality, relevant to Q1

This was not in the reviewer's framing and it may matter more than throughput.

**Whole-reparse cannot produce per-range certification.** With a full reparse
every keystroke, nothing survives by construction — there is no reuse record, so
no basis on which to claim a distant paragraph is still certified. Recovering it
would require diffing the old and new trees on every keystroke, which is itself
document-proportional work on the interactive path (RFC 024 §6.3 forbids it).

So incrementality is not only about *speed*. It is what lets the editor **keep
styling the 99% of the document an edit did not touch**. Under the certification
model that is a correctness-of-experience property, not an optimisation — and it
is an axis on which whole-reparse may lose decisively regardless of how fast it
parses.

**Q1's disposable challenge must therefore measure this too:** not just
end-to-end latency, but *how much of the document remains certified after a
keystroke*. If whole-reparse invalidates everything every time, it fails the
§4.4 rendering model no matter what the clock says.
