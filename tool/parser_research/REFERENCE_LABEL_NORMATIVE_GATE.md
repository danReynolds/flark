# Reference-label normative gate

Status: **HOLD — one shared spec-owned service is required before reference
publication or inline reference resolution can be called exact**, 2026-07-18.

## Why this gate exists

Flark's selected syntax profile, not any donor implementation, is normative.
For reference labels the distinction is already observable:

- [CommonMark 0.31.2](https://spec.commonmark.org/0.31.2/) limits the contents
  of a link label to 999 characters;
- the pinned Comrak scanner counts bytes and admits 1,000 of them, so it both
  rejects valid long multibyte labels and accepts an invalid 1,000-character
  ASCII label; and
- Comrak's label normalizer uses Rust's broad `char::is_whitespace`, while the
  specification names spaces, tabs, and line endings. That changes labels
  containing NBSP and other Unicode whitespace.

The current Pulldown-derived inline proof counts Unicode scalar values, but it
also uses broad Unicode whitespace. The older oversized-line and owned-parser
prototypes contain their own different approximations. None of those helpers
is eligible to become a second production authority.

## Selected semantic contract

One Flark-owned `ReferenceLabelService` is used by both:

1. the block controller's reference-definition finalizer; and
2. the inline service's full, collapsed, and shortcut reference lookup.

The service operates over an authenticated logical-projection cursor and
returns the exact logical label cuts plus a normalized key. It does not decide
whether a block is a definition or whether an inline bracket is a link; those
decisions remain with their respective grammar stages.

Calling a shared helper only when inserting a definition into the reference
table is insufficient. The inline scanner must build its lookup key through
the same accumulator. The current Pulldown-derived proof still has a local
`push_normalized(char::is_whitespace)` path and therefore remains HOLD until
that actual scan path, not just `ReferenceTable::define`, uses this service.

For the pinned profile it must:

- count Unicode scalar values in the raw content between brackets, across
  arbitrary UTF-8 refill boundaries; CRLF therefore contributes two
  characters to the limit even though it is one line ending;
- reject after the 999th scalar, with brackets excluded from the count;
- preserve backslashes for label matching; recognition separately uses them
  to determine whether a bracket is escaped;
- reject an unescaped nested `[` and end at the first unescaped `]`;
- perform the pinned full Unicode case fold;
- trim and collapse only U+0020 SPACE, U+0009 TAB, and logical line endings;
- normalize a source CRLF as one line ending and therefore one collapsed
  ASCII space, without losing its two-character contribution to the limit;
- return no key for a label containing only that specified whitespace.

VERTICAL TAB, FORM FEED, NBSP, EM SPACE, and other characters outside the
named set remain ordinary label characters unless the selected profile is
deliberately versioned to say otherwise. This also differs from the cmark
reference helper, whose ASCII `isspace` table includes vertical tab and form
feed.

## Bounded implementation shape

```text
ReferenceLabelAccumulator
  binding: syntax profile + request + source/projection identity
  scalar_count: 0..999
  utf8_decoder: bounded partial scalar
  raw: at most 999 UTF-8 scalars
  whitespace/case-fold continuation

finish(exact logical end)
  -> ReferenceLabelKey(normalized bytes, logical raw cuts, profile version)
```

The logical cursor must expose the source-provenance character contribution of
each logical unit. In particular, a canonical LF produced from CRLF carries a
raw contribution of two. A cursor containing only canonical bytes and logical
UTF-16 length is insufficient for this rule; the driver may not guess from an
absolute source offset.

The raw UTF-8 retention is bounded by 999 four-byte scalars. Normalized output
is also fixed-envelope work: its maximum expansion is derived from the pinned
Unicode case-fold table, preflighted before mutation, and exhaustively tested
against every scalar in that table. No caller supplies a normalized `String`,
a scalar count, or an absolute physical range.

The first prototype may retain the bounded raw label and normalize at `finish`.
Production extraction may fuse decoding and normalization if measurement says
that matters, but it must keep the same service and tests. Destination and
title payloads are not label-service input and remain source-backed.

## Required executable receipts

The gate is green only when the same implementation passes all of these:

- 0, 1, 998, 999, and 1,000 ASCII-scalar boundaries;
- 998, 999, and 1,000 two-, three-, and four-byte scalar boundaries with
  one-byte source refills;
- an incomplete or invalid UTF-8 scalar at every possible refill boundary;
- escaped brackets/backslashes at the limit;
- SPACE, TAB, LF, CRLF, VERTICAL TAB, FORM FEED, NBSP, EM SPACE, and
  mixed-whitespace counterexamples;
- 997 characters plus CRLF (accepted) versus 998 plus CRLF (rejected), while
  both normalize the line ending to one space;
- full case-fold expansion examples such as sharp-S, plus exhaustive agreement
  with the pinned Unicode fold table;
- block-definition and inline-use keys that are byte-identical for the same
  logical label, including non-contiguous quote/list projections;
- a definition/lookup pair for every full-fold and whitespace
  counterexample, proving the actual inline scan path rather than a direct
  table-helper call;
- clean parse versus every-fuel/every-refill equivalence; and
- cancellation, stale source, crossed request, and replay failures before any
  occurrence or winner-root mutation.

Unmodified Comrak, Pulldown, and cmark-gfm remain differential peers. Expected
donor disagreements are recorded as adjudicated fixtures rather than changed
until every donor agrees. A donor upgrade may remove a disagreement, but cannot
silently change the profile key or its persisted version.

## Architectural consequence

This finding strengthens the Flark-owned controller direction. A narrow donor
scanner is useful only behind the normative service boundary; copying its
label limit and normalizer would make donor behavior an accidental second
specification. The shared service is small, bounded, and independently
certifiable, so replacing the divergent helpers reduces rather than adds
architecture.
