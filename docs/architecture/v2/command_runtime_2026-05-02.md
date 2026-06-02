# Sovereign v2 Command Runtime

Status date: 2026-05-02

## Scope

This note covers the first command runtime slice under
`lib/src/v2/core/command`. It is intentionally markdown-agnostic. Markdown
commands will build on this layer in Phase 2.

## Current Contract

- `SovereignCommand<TPayload>` identifies a typed command by stable string id.
- `SovereignCommandContext<TPayload>` passes immutable editor state, command,
  and payload to handlers.
- `SovereignCommandResult` can be:
  - handled, optionally with a transaction;
  - not handled, allowing lower-priority handlers to run;
  - rejected, stopping dispatch with a typed reason string.
- `SovereignCommandRegistry` is immutable. Registering a handler returns a new
  registry.
- Handlers are ordered by integer priority. Higher priority runs first.
- Handlers return transactions rather than mutating editor state.

## Why This Shape

The registry follows the same core rule as transactions: behavior returns data.
That keeps command handling testable outside Flutter and lets toolbar actions,
keyboard shortcuts, paste policies, table commands, and extension commands use
one dispatch path.

## Next Command Slices

1. Add canonical command ids for source insertion and selection replacement.
2. Add command helpers for applying a returned transaction to state and history.
3. Add markdown inline style command models.
4. Add command capability checks that return rejected/no-op results rather than
   throwing for normal unsupported states.
5. Add extension-owned commands after core markdown commands prove the registry
   shape.

## Open Questions

- Whether command ids should be structured strings only or backed by a typed
  namespace object.
- Whether rejection reasons should become typed enum/value objects before
  public export.
- Whether command dispatch should return a richer result containing both the
  transaction and the next state once the engine/runtime object exists.
