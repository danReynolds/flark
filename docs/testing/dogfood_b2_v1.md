# D0 reviewed B2 and out-of-scope ledger

This ledger is part of the exact-candidate D0 completion receipt. Items here
must be safe, bounded, and outside the frozen D0 interaction denominator. Any
finding that affects a frozen scenario is B1 and cannot be waived here.

## Accepted for D0

- `B2-ARCH-01`: `FlarkEditorController` and the Rust runtime document module
  remain large. D0 has one pending-presentation state and one retirement
  lifecycle; file decomposition is deferred until after Dan's dogfood.
- `B2-WIRE-01`: legacy literal-envelope and edit-cell wire record types remain
  ABI adapters into the sealed Core pending-presentation model. Wire cleanup is
  deferred; no second host authority path is permitted.
- `B2-SYNTAX-01`: the parser-owned `SYNTAX-10` dependency plan is frozen to the
  declared fixture. Generalizing it to arbitrary nested delimiter graphs is
  outside D0.
- `B2-SHELL-01`: the dogfood app is a preset workbench. Arbitrary file
  open/save, autosave, and edited-source persistence across restart are outside
  D0; reopening a preset must initialize its pristine source without stale
  session state.
- `B2-PLATFORM-01`: mobile interaction, physical-device IME, accessibility,
  touch, themes, notarization, and public packaging are alpha/release work, not
  Mac D0 evidence.

## Reopen rule

Reclassify an item as B0/B1 immediately if it produces corruption, a
crash/hang/fault, torn source-selection-caret identity, a wrong in-denominator
paint/action, or a repeatable D0 performance-budget failure.
