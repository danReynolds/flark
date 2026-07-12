# IME device-test protocol

Manual pass for real-keyboard behavior the simulated suite
(`test/v2/flutter/flark_ime_input_test.dart`) cannot vouch for. It also gates
the Stage-3 convergence work in `doc/architecture/live_edit_intent_pipeline.md`.
Run it in the example app (`cd example && flutter run -d <device>`), in the
Scratch document, live-rendered mode.

**Known defect to watch everywhere styled input appears** (pinned by the
skipped test in the automated suite): a keystroke that creates or relocates
inline delimiters mid-composition (first character with a style armed; first
character re-entering a run after a committed trailing space) clears the
composing region and pushes a transient raw-marker editing state to the IME.
Expected on-device symptom: the suggestion strip resets / kana composition
cancels after that keystroke; possibly a one-frame `**` flash. Record whether
each keyboard recovers or corrupts.

## Matrix

| # | Platform | Keyboard / input source | Extra settings |
| - | -------- | ----------------------- | -------------- |
| 1 | Android  | GBoard                  | predictive + autocorrect ON |
| 2 | Android  | Samsung Keyboard        | predictive ON |
| 3 | Android  | GBoard Japanese (12-key & QWERTY romaji) | — |
| 4 | Android  | GBoard Korean           | — |
| 5 | iOS      | default English         | predictive + autocorrect ON |
| 6 | iOS      | Japanese — Romaji and Kana | — |
| 7 | iOS      | Korean                  | — |
| 8 | macOS    | Hiragana / Pinyin input source | hardware keyboard |
| 9 | Android + iOS | voice dictation    | mic button on keyboard |

Run S1–S9 on every row where the script applies (CJK rows: S3–S5; voice: S10).

## Scenarios

Each scenario ends with the same two checks, mirroring the automated
invariants: **(a)** the rendered text is exactly what you typed (no visible
`**`/`` ` ``/`~~`, nothing missing or doubled); **(b)** export round-trip —
copy the markdown out (or read the autosave) and paste it into a fresh
document/preview: it must render identically.

- **S1 — predictive words.** Empty doc. Type `hello`, accept the suggestion
  (or space-commit), then `world` the same way. Expect `hello world `.
- **S2 — predictive words, bold armed.** Empty doc, toggle Bold (toolbar or
  Cmd/Ctrl+B), then S1. Expect bold "hello world" rendered without markers;
  exported source exactly `**hello world** ` (one run, space outside the
  delimiters). Watch the known defect: does composition survive the first
  keystroke of each word?
- **S3 — autocorrect.** Type `teh` then space. Expect `the `; no doubled
  letters, caret after the space.
- **S4 — Japanese conversion.** Type romaji `kan`, convert to `感`, commit.
  Once in plain text; once with the caret placed mid-word inside an existing
  bold run (expect `**bo感ld**`-shaped source). Composition must never be
  dropped mid-conversion; the candidate window must track the caret.
- **S5 — Korean jamo.** Type `ㅎㅏㄴ` → `한`, continue `ㄱㅡㄹ` → `한글`. Each
  keystroke rewrites the visible cluster; expect no orphaned jamo.
- **S6 — styled trailing edge.** Doc `**hello**` (paste, or type with Bold).
  Tap at the end of the word, compose `wo`, commit with a space, then type
  `x`. Expect display `hellowo x`, source `**hellowo x**` (space outside,
  re-entry on `x`).
- **S7 — backspace mid-composition.** Compose `hel`, backspace once while
  still composing, commit. Expect `he`; the composition shrinks rather than
  cancels.
- **S8 — inline code span.** Doc `` a `code` b ``. Tap at the end of `code`,
  compose `x`, commit with a space. Expect source `` a `codex ` b `` — space
  inside the span, backticks unmoved.
- **S9 — replace a selection by composing.** Doc `hello world`. Double-tap to
  select `world`, then type/compose `wonder` and commit with a space. Expect
  `hello wonder ` — no remnant of the old word, no marker damage.
- **S10 — voice dictation.** Dictate "hello world" into plain text, then with
  Bold armed. Dictation arrives as large composing-then-committed chunks;
  expect the same outcomes as S1/S2.
- **S11 — code-fence flows under a CJK IME (macOS/Android).** With row 3/8
  active, run the fence flows from `example/test/widget_test.dart` manually:
  type ```` ``` ```` + Enter, a language shortcut on the first body line, body
  Enters, and a typed closing fence. Watch for dropped composition, doubled
  characters after auto-close echoes, and resync loops (GBoard).

## Expected-failure bookkeeping

S2/S6/S10-bold currently sit on the known defect: log the observed keyboard
behavior (composition cancelled? text still correct?) rather than pass/fail
on composition survival alone. Document corruption or a bad export is always
a failure.

## What to record on failure

1. Device + OS version; keyboard name, version, and the settings toggles.
2. The exact keystroke/candidate sequence (slow screen recording:
   `adb shell screenrecord /sdcard/ime.mp4`; iOS Control-Center recording).
3. The rendered text (screenshot) vs the exported markdown (copy it out
   immediately, before moving the caret).
4. Android: `adb shell settings get secure default_input_method`, and
   `adb logcat -s ImeTracker InputMethodManagerService` around the repro;
   `flutter logs` for Dart-side output.
5. iOS/macOS: Console.app filtered to the app process (or the Xcode console)
   around the repro.
6. If possible, translate the repro into a `TextEditingValue` sequence and
   add it to `flark_ime_input_test.dart` via `_composeAndCommit` — the
   comment on the skipped armed-wrap test shows the shape.

File findings against the recognizer matrix in
`doc/architecture/live_edit_intent_pipeline.md`; convergence candidates #12
and #15 must not land until rows 1–8 pass S11 clean.
