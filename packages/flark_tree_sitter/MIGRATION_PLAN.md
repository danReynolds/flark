# Finish the single-engine migration

Authorized by the owner on 2026-09-07: remove the parallel highlighter and
language-sensitive fallback. Preserve existing language choices and Automatic.

1. [x] Add pinned upstream grammars for the remaining catalog. Qualify highlighting
   and shared edit queries with positive, literal, incomplete and selection cases.
2. [x] Move the catalog, aliases and bounded Automatic detection to this package.
   Use parser/query evidence; ambiguous input may stay plain, manual choice wins.
   Check changing-source detection cost as well as correctness.
3. [x] Remove `highlight`, its cache/registration, and Flark's lexical indenter.
   Core owns source/selection/history and generic whitespace only. Hosts opt
   into the shared service and use its catalog for all language decisions.
4. [x] Run native/Wasm parity, host source/caret/container/undo and browser first-paint
   checks. Dogfood Automatic, typed indent/outdent, selection, paste and source
   mode. Record source/asset hashes and refresh the preserved user preview.

Review at each boundary. A second parser integration, hidden behavior loss,
false detection on prose, startup/main-thread cost or grammar-specific host
branches must be resolved before completing this migration. Physical-device,
IME and sustained full-frame performance gates remain distinct.

Completed on 2026-09-07. [SINGLE_ENGINE_REVIEW.md](SINGLE_ENGINE_REVIEW.md) records
scope, boundary findings, capability limits and the final receipt. All existing
choices use one engine; no new twenty-language promise is implied. The user
preview was refreshed and its saved source compared exactly. Sustained frame,
physical-device and IME qualification remain the separate gates named above.
