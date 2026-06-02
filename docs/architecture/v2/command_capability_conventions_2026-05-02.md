# Sovereign v2 Command Capability Conventions

Status date: 2026-05-02

## Purpose

Commands need predictable behavior when a feature is unavailable, irrelevant,
or invalid for the current selection. This note defines the current result
conventions for `SovereignCommandResult`.

## Result Meanings

### Handled With Transaction

Use when the command produced a source edit.

Examples:

- insert text;
- wrap selected text in inline markers;
- unwrap inline markers;
- future list/quote/table transforms.

### Handled Without Transaction

Use for a valid no-op where the command intentionally consumed the request but
source text should not change.

Examples:

- future command asks to apply a style that is already active and policy says
  keep it;
- future selection-only command updates runtime state outside source text.

### Not Handled

Use when a handler does not own the command in the current context and lower
priority handlers should get a chance.

Examples:

- an extension inspects a command but delegates to the default handler;
- a table-specific handler sees the selection is outside a table and allows the
  normal paragraph handler to run.

### Rejected

Use when the command owns the request but the current state is unsupported or
invalid. Rejection stops dispatch.

Examples:

- collapsed inline-style toggle before active-mark state exists;
- malformed table selection;
- unsupported media source;
- parser profile does not support a requested syntax extension.

## Current Rule

Normal unsupported editing states should return `rejected` or `notHandled`;
they should not throw. Throws are reserved for programmer errors and invariant
violations, such as overlapping source operations.
