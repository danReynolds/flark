# IME device matrix — runner's script (G1)

**Audience:** whoever is holding the phones. No prior context assumed.
**Time:** 60 min per phone for Lane 1, +30 min for Lane 2, +25 min once on a Mac
for Lane 3. Lane 1 alone is a valid, useful result — do not skip cases to reach
the end of Lane 2.
**Output:** a filled copy of
[`ime_device_matrix_recording_sheet.md`](ime_device_matrix_recording_sheet.md),
plus screen recordings for anything that is not a clean pass.

This supersedes [`ime_device_protocol.md`](ime_device_protocol.md) as the thing
you execute. That file remains the record of *why* these scenarios exist; this
file is *how* to run them. Where they disagree, this file wins (§0.3 lists the
disagreements and how they were resolved).

---

## 0. Read this before touching a device

### 0.1 What this gate decides

Two independent things, in one pass:

1. **Part A (S1–S11)** — does today's editor (v2) survive real keyboards? A
   failed **S11 across rows 1–8** is a written-in-ink trigger to reopen the
   *input* architecture (`docs/architecture/architecture_position_2026-07-12.md`
   §4 W1).
2. **Part B (N1–N10)** — what does Flutter's own `EditableText` handle correctly
   today, across the behaviours
   [RFC 024](../architecture/rfc/rfc_024_bounded_inframe_markdown_engine.md) §7
   names? RFC 024 §8 D1 already retired v2 as a product; here it is only a
   *reference implementation*. Part B results become the acceptance suite the
   new input surface (gate G4) must match or beat.

So: **Part A failures are bugs. Part B results are measurements.** A Part B case
that fails on v2 is not a v2 bug report — it is a recorded baseline. Write down
what happened either way; do not editorialise.

### 0.2 The one rule that overrides everything

> **The Markdown source is the truth. A wrong source is always a failure, even
> if the screen looks right.**

Which is why §2.4's *Show source* sheet is mandatory equipment. A case where the
display is correct and the source is wrong is the single most valuable finding
this pass can produce, and it is invisible without that sheet.

### 0.3 Where the old protocol was ambiguous, and what was decided

Recorded so a reviewer can tell a decision from a drift.

| Ambiguity in `ime_device_protocol.md` | Resolution used here |
| --- | --- |
| "Run S1–S9 on every row where the script applies (CJK rows: S3–S5; voice: S10)" — S3 is autocorrect, not CJK; S10/S11 are outside the stated range; no row→scenario map exists | §3.2 gives an explicit row × scenario applicability table |
| S11 is titled "under a CJK IME (macOS/Android)" (rows 3, 4, 8) but the architecture position makes "S11 across rows **1–8**" the reopen trigger | S11 runs on **all rows 1–8**. The trigger cannot be evaluated otherwise. CJK rows are the harshest and are Lane 1; Latin rows are Lane 2 |
| Check (b) says "copy the markdown out (or read the autosave)" — the example app has **no** source view, **no** export, and **no** autosave; `Cmd+A`+copy yields projected text, not source (measured, §5 N3) | Appendix A adds a *Show source* sheet to the example app. Without it, check (b) is not executable and Part A is not runnable |
| S11 says "run the fence flows from `example/test/widget_test.dart` manually" — a 1,720-line test file | §4 S11 spells out five literal keystroke sequences with the exact expected source |
| Row 9 is "Android + iOS voice dictation" as one row | Recorded as 9a (Android) and 9b (iOS); it is still "row 9" for the trigger |
| No pass/fail vocabulary, so two runners would classify differently | §6 defines eight result codes; the sheet accepts only those |

### 0.4 Known non-defects — do not report these

- **Yellow-and-black striped overflow bars on the landing page hero** at ~390 pt
  width (iPhone 13/14/15 non-Max, and similar). Pre-existing example-app layout
  bug in the marketing hero (`example/lib/main.dart` around lines 1027 and
  1070), reproduced on unmodified `main.dart` in a widget test at 390×844. It is
  above the editor and has nothing to do with the editor. Ignore it.
- **iOS Smart Punctuation** turning `"` into `"` or `--` into `—`. That is iOS
  rewriting your keystrokes before flark sees them. Record it in Notes if it
  changes an expected source string; it is not an editor defect.
- Debug-build jank. Everything here is a *correctness* pass. Frame timing is
  gate G2 and uses a different harness (`example/lib/perf_harness.dart`).

---

## 1. Prep (a developer does this once, before the runner arrives — ~45 min)

1. **Toolchain.** Flutter on `PATH` (this program used 3.44.4 at
   `/Users/dan/Coding/flutter_arm64/bin`) and a Rust toolchain via
   [rustup](https://rustup.rs). `hook/build.dart` compiles the Comrak bridge
   during `flutter run` and will `rustup target add` the phone triples itself;
   no manual native build step is needed. Budget 5–15 min for the first build.
2. **Apply the *Show source* patch** — Appendix A. Verify with
   `cd example && flutter analyze lib` (expect `No issues found!`) and
   `flutter test test/widget_test.dart` (expect `39` passing). Both were run and
   passed with this patch applied.
3. **Install to both phones** and confirm the app opens:
   ```
   cd example
   flutter devices                       # copy the device id
   flutter run -d <device-id> -t lib/main.dart
   ```
   Leave `flutter run` attached — the runner will want hot restart (`R`) to
   reset a wedged document, and `flutter logs` output if something crashes.
4. **Do the §2 device setup** for each phone so the runner starts with every
   keyboard already installed. This is the single biggest time sink and it is
   pure setup — do not make the runner do it.
5. **Hand over:** the two phones, this file, a blank copy of the recording
   sheet, and how to start a screen recording on each.

---

## 2. Device setup

### 2.1 Android (rows 1, 2, 3, 4, 9a)

| What | Where |
| --- | --- |
| Gboard English + autocorrect | Settings → System → Languages & input → On-screen keyboard → Gboard → **Text correction**: Auto-correction **ON**, Show suggestion strip **ON**, Next-word suggestions **ON** |
| Gboard Japanese | Gboard → Languages → Add keyboard → 日本語 → enable **both** *12 キー* (flick) and *QWERTY* (romaji) layouts |
| Gboard Korean | Gboard → Languages → Add keyboard → 한국어 → **두벌식 (2-Bulsik)** |
| Samsung Keyboard (row 2) | Samsung devices only. Settings → General management → Samsung Keyboard settings → **Predictive text ON** |
| Voice typing (row 9a) | Settings → System → Languages & input → On-screen keyboard → **Google voice typing** enabled; mic key appears on Gboard |
| Switching keyboards | 🌐 globe key next to the space bar, or long-press the space bar |
| Which IME is live | `adb shell settings get secure default_input_method` |

If no Samsung device is available, mark every row-2 cell **`Blocked`**. Do not
substitute another keyboard and call it row 2.

### 2.2 iOS (rows 5, 6, 7, 9b)

| What | Where |
| --- | --- |
| English predictive + autocorrect | Settings → General → Keyboard → **Auto-Correction ON**, **Predictive ON** |
| Japanese (Romaji **and** Kana) | Settings → General → Keyboard → Keyboards → Add New Keyboard → **日本語** → enable both *ローマ字* and *かな* |
| Korean | Add New Keyboard → **한국어** |
| Dictation (row 9b) | Settings → General → Keyboard → **Enable Dictation ON** |
| Switching keyboards | 🌐 globe key, bottom-left; long-press it for the picker |
| Smart Punctuation | Leave at its default. See §0.4 |

### 2.3 macOS (row 8 — Lane 3 only)

System Settings → Keyboard → Text Input → Input Sources → **Edit…** → **+**:
add **Japanese** (Romaji / Hiragana) and **Chinese, Simplified → Pinyin –
Simplified**. Switch with `Ctrl+Space` or the menu-bar input menu.
Run with `cd example && flutter run -d macos -t lib/main.dart`.

### 2.4 Reaching the editor, and your four instruments

On launch the app shows a marketing landing page. **Scroll down** past the
headline until you see a card with a title bar reading *Flark Markdown* and a
row of chips: `Sample` `Article` `Tables` `Scratch`. (At phone width the top-nav
"Playground" link is hidden — scrolling is the only route.) Tap **`Scratch`** to
get an empty document. Tap once in the white area below the toolbar to focus the
editor and raise the keyboard.

**Do not tap `Expand`.** Fullscreen mode drops the toolbar, the caret readout
and the *Show source* button — you need all three.

Your instruments, all in that one card:

| # | Instrument | Where | Reads |
| --- | --- | --- | --- |
| 1 | **Rendered text** | the editor itself | what the user sees |
| 2 | **Parse badge** | title bar, right | `Parsed` (teal) / `Parsing` (amber). Wait for `Parsed` before reading the source |
| 3 | **Caret readout** | footer under the editor | `Caret <n> · <m> chars`, or `Caret <n> · <k> selected` when a selection exists. **These are *source* offsets, not screen positions and not display offsets** — so with markers hidden the number is larger than the visible character count (caret at the end of a displayed `hello` that is really `**hello**` reads `Caret 7`, not 5). This is how you observe model-range selection |
| 4 | **Show source** | toolbar, far right, `{ }` icon (Appendix A) | the exact Markdown, with `·` for every space and `⏎` for every newline, plus a *Copy raw source* button |

To record a source: open *Show source*, screenshot it, and where the sheet is
short, transcribe it into the sheet verbatim including every `·` and `⏎`.

**Reset between cases:** tap `Scratch` again. If the editor stops responding,
hot-restart from the attached `flutter run` with `R` and note it.

---

## 3. The matrix

### 3.1 Rows (numbering is load-bearing — do not renumber)

| # | Platform | Keyboard / input source | Settings |
| - | -------- | ----------------------- | -------- |
| 1 | Android | Gboard English | predictive + autocorrect ON |
| 2 | Android | Samsung Keyboard | predictive ON |
| 3 | Android | Gboard Japanese (12-key **and** QWERTY romaji) | — |
| 4 | Android | Gboard Korean | — |
| 5 | iOS | default English | predictive + autocorrect ON |
| 6 | iOS | Japanese — Romaji **and** Kana | — |
| 7 | iOS | Korean | — |
| 8 | macOS | Hiragana / Pinyin input source | hardware keyboard |
| 9a | Android | voice dictation | mic key |
| 9b | iOS | voice dictation | mic key |

Rows 3 and 6 are each two sub-runs (romaji layout, then kana/flick layout). Run
Lane 1 on both layouts; Lane 2 on romaji only.

### 3.2 Row × scenario applicability

`•` = run it. `—` = not applicable. Lane in §3.3.

| | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9a | 9b |
|---|---|---|---|---|---|---|---|---|---|---|
| S1 predictive words | • | • | — | — | • | — | — | — | — | — |
| S2 predictive + bold armed | • | • | — | — | • | — | — | — | — | — |
| S3 autocorrect | • | • | — | — | • | — | — | — | — | — |
| S4 Japanese conversion | — | — | • | — | — | • | — | • | — | — |
| S5 Korean jamo | — | — | — | • | — | — | • | — | — | — |
| S6 styled trailing edge | • | • | • | • | • | • | • | • | — | — |
| S7 backspace mid-composition | • | • | • | • | • | • | • | • | — | — |
| S8 inline code span | • | • | • | • | • | • | • | • | — | — |
| S9 replace selection by composing | • | • | • | • | • | • | • | • | — | — |
| S10 voice dictation | — | — | — | — | — | — | — | — | • | • |
| S11 fence flows (a–e) | • | • | • | • | • | • | • | • | — | — |
| N1–N10 (Part B) | see §5 — N4/N5/N9-desktop are row 8 only; the rest run on rows 1, 5 and 8 |

### 3.3 Lanes — what to do with the time you have

**Lane 1 — the hour (per phone).** Highest information per minute.
S2 · S6 · S8 · S9 · (S4 on rows 3/6, S5 on rows 4/7) · S11a–S11c · **N8** · N3 · N7.
N8 is the case RFC 024 §7 flags as most likely to break; if you run out of time,
run N8 anyway.

**Lane 2 — completeness (+30 min per phone).** S1 · S3 · S7 · S11d · S11e ·
S10 (row 9) · N1 · N2 · N6 · N10.

**Lane 3 — the Mac (25 min, once).** Row 8: S4 · S6 · S8 · S9 · S11a–e, then
N4 · N5 · N9 · N10. Pointer and hardware-keyboard cases only exist here.

---

## 4. Part A — v2 baseline scripts (S1–S11)

**Every case ends with the same two checks:**

- **(a) display** — the rendered text is exactly what you meant to type. No
  visible `**`, `` ` ``, `~~`, `#`, `>` or `-` markers. Nothing missing,
  nothing doubled.
- **(b) source** — open *Show source* and compare to the Expect-source line,
  character for character, `·` and `⏎` included.

Expectations carry a confidence tag:
**[pinned]** = an automated test asserts this exact string today (file named);
**[stated]** = the old protocol asserts it, nothing pins it;
**[measured]** = measured during preparation of this runbook, quoted verbatim.

Notation: `·` = one space, `⏎` = newline, `⌫` = backspace.

---

### S1 — predictive words

1. `Scratch`. Tap into the editor.
2. Type `hello`. Accept the keyboard's suggestion (or press space).
3. Type `world`. Accept the same way.

- Expect display: `hello world ` (trailing space)
- Expect source: `hello·world·`  **[pinned:
  `packages/flark_flutter/test/v2/flutter/flark_ime_input_test.dart`,
  "predictive composition commits two plain words"]**

---

### S2 — predictive words with Bold armed *(Lane 1 — the historical defect)*

1. `Scratch`. Tap into the editor. Do **not** type yet.
2. Tap the **B** button in the toolbar (or `Cmd/Ctrl+B` on row 8). It should
   light up. Focus must stay in the editor — the toolbar buttons are built not
   to steal it.
3. Type `hello`, accept the suggestion / press space.
4. Type `world`, accept the same way.

- After step 3 — display `hello `, source `**hello**·`
- After step 4 — display `hello world `, source `**hello·world**·`
  **[pinned: same file, "predictive composition with strong armed commits the
  canonical \*\*word\*\* shape and re-enters on the next word"]**
- One styled run, not two. `**hello** **world**` is a failure.
- **Watch the keyboard, not just the screen**, on the *first* character of each
  word: the suggestion strip must not reset, kana composition must not cancel,
  and no `**` may flash. This is the formerly-defective path — it is fixed in
  the simulated suite and this pass is what confirms it on real IMEs.

---

### S3 — autocorrect

1. `Scratch`. Type `teh` then space.

- Expect display `the `, source `the·` **[stated]**
- Caret sits after the space (readout `Caret 4`). No doubled letters
  (`thehe`, `tthe`).

---

### S4 — Japanese conversion *(rows 3, 6, 8)*

**S4a — plain text.** `Scratch`, type romaji `kan`, convert to `感`, commit.

- Expect display `感`, source `感` **[pinned: same file, "Japanese conversion
  rewrites the composing region wholesale in plain text"]**

**S4b — inside a bold run.** `Scratch`, arm **B**, type `bold`, tap **B** again
to disarm, then tap between `o` and `l` (readout should show `Caret 4`). Type
romaji `kan`, convert to `感`, commit.

- Expect display `bo感ld`, source `**bo感ld**` **[pinned: same file, "Japanese
  conversion composes inside an existing strong run"]**
- Composition must never be dropped mid-conversion. The candidate window must
  follow the caret, not jump to the start of the block.

---

### S5 — Korean jamo *(rows 4, 7)*

1. `Scratch`. Type the jamo keys `ㅎ` `ㅏ` `ㄴ` — the cluster should rewrite in
   place: `ㅎ` → `하` → `한`.
2. Continue `ㄱ` `ㅡ` `ㄹ` → `글`.

- Expect display `한글`, source `한글` **[pinned: same file, "Korean jamo
  composition rewrites the cluster per keystroke"]**
- No orphaned jamo (`ㅎㅏㄴ글`), no doubled cluster (`한한글`).

---

### S6 — styled trailing edge

1. `Scratch`. Arm **B**, type `hello`, disarm **B**. Confirm via *Show source*:
   `**hello**`.
2. Tap at the very end of the word. The readout counts the hidden markers, so
   expect roughly `Caret 7`, not `Caret 5`.
3. Compose `wo` and commit with a **space**.
4. Then type `x`.

- After step 3 — display `hellowo `, source `**hellowo**·` (the space commits
  *outside* the closing marker)
- After step 4 — display `hellowo x`, source `**hellowo·x**` (the `x` re-enters
  the run and pulls the space back inside)
  **[pinned: same file, "composition at a strong run trailing edge commits the
  space outside and stays armed"]**

---

### S7 — backspace mid-composition

1. `Scratch`. Compose `hel` — do not commit.
2. Press ⌫ once while still composing.
3. Commit.

- Expect display `he`, source `he` **[pinned: same file, "backspace
  mid-composition shrinks the composing region before commit"]**
- The composing region must **shrink**, not cancel. If the underline under the
  composing text disappears at step 2, that is `Fcomp`.

---

### S8 — inline code span

1. `Scratch`. Type the literal characters `` a `code` b `` — backticks and all.
   (The toolbar's `<>` button makes a *fenced block*, not an inline span; the
   inline-code command is `Cmd/Ctrl+E`, hardware keyboards only.) Confirm via
   *Show source*: ``a·`code`·b``. The display should read `a code b` with the
   backticks hidden.
2. Tap right after the `e` of `code` in the display. Readout ≈ `Caret 7` (it
   counts the hidden opening backtick).
3. Compose `x`, commit with a **space**.

- Expect display `a codex  b` (two spaces before `b`)
- Expect source: ``a·`codex·`·b`` — the space stays **inside** the backticks and
  the backticks do not move **[pinned: same file, "composition inside an inline
  code span keeps whitespace inside and backticks untouched"]**

---

### S9 — replace a selection by composing

1. `Scratch`. Type `hello world`.
2. Double-tap `world` to select it. Footer should read `… · 5 selected`.
3. Compose `wonder`, commit with a space.

- Expect display `hello wonder `, source `hello·wonder·` **[pinned: same file,
  "composing over a widget-made selection replaces exactly the selected word"]**
- No remnant of `world`, no marker damage.

---

### S10 — voice dictation *(row 9a / 9b)*

1. `Scratch`. Tap the mic key. Dictate "hello world". Stop dictation.
2. `Scratch` again, arm **B**, dictate "hello world", stop.

- Run 1 — expect the S1 outcome: display `hello world`, source `hello·world`
  (dictation may or may not append a trailing space — record what you get).
- Run 2 — expect the S2 outcome: one bold run, source `**hello·world**` shape.
  **[stated]**
- Dictation arrives as large composing-then-committed chunks; that is the point
  of the case.

---

### S11 — code-fence flows *(rows 1–8, Lane 1 = a,b,c)*

**Keyboard note for CJK rows:** a backtick is usually unreachable from a
Japanese 12-key or Korean layout. Switch to the Latin/QWERTY layout with 🌐 for
the backticks, then switch **back** to the CJK layout to type the body text.
That layout switch mid-document is itself part of what this case tests — do not
avoid it by staying on Latin.

Each sub-case starts from a fresh `Scratch`. In the blocks below, everything
between the `~~~` lines is what you literally type; `⏎` means press Enter.

**S11a — fence opens on the third backtick.**

~~~
```
~~~
- Expect: a code-block region appears the moment the third backtick lands; the
  backticks are **not** visible; a language button appears.
- Expect source (whitespace-visible): `` ```⏎ ``
  **[pinned: `example/test/widget_test.dart`, "scratch renders a fence region
  after triple backticks"]**

**S11b — language on the opening line survives fast typing.** Type straight
through, no pauses:

~~~
```dart ⏎ foo
~~~
On CJK rows type the body word through the IME (romaji-convert a short word
instead of `foo`) and record what you actually typed.
- Expect source: `` ```dart⏎foo `` **[pinned: same file, "scratch keeps fast
  typed fence language on the opening line"]**

**S11c — the closing fence lands outside the body.** Type straight through:

~~~
```dart ⏎ foo ⏎ ``` ⏎ ⏎ ⏎ abcdef
~~~
- Expect source: `` ```dart⏎foo⏎```⏎⏎⏎abcdef `` **[pinned: same file, "scratch
  keeps fast typed fence closing outside the code body"]**
- `abcdef` must render as a normal paragraph *below* the fence, not as code.

**S11d — body text on the opening line.**

~~~
```fffffff
~~~
- Expect source: `` ```⏎fffffff `` — the `fffffff` moves down into the body, and
  the language button is present **[pinned: same file, "scratch opens a fence as
  soon as the third backtick is typed"]**

**S11e — backspace removes a newly opened empty fence.** Type three backticks,
then press ⌫ once.
- Expect: the fence disappears and you are back to an empty document.
  **[pinned: same file, "scratch backspace removes a newly opened empty code
  fence"]**

**Across all of S11, watch for:** dropped composition when the fence auto-closes
under you; doubled characters after the auto-close echo (`ddart`, `foofoo`); and
a **resync loop** — Gboard's suggestion strip flickering continuously, or typing
that lags then catches up in a burst. Any of those is the S11 failure that trips
the architecture trigger. Record it as `Fcomp` or `Fperf` and **film it**.

---

## 5. Part B — RFC 024 §7 acceptance cases (N1–N10)

These are the behaviours the *new* input surface must handle (gate G4). Run them
on v2 to capture the baseline. Some are **expected to fail on v2** — that is the
measurement, not a bug report.

**Setup for the scroll cases (N1, N2, N8).** You need a document taller than the
editor viewport (the editor is a fixed ~480 pt box on a phone, and it scrolls
internally). Fastest route: tap `Article`, tap *Show source* → **Copy raw
source**, then put the caret at the end of the document and paste 4–5 times.
Confirm the footer's char count grew and that the editor now scrolls.

---

### N1 — drag-select across blocks, with autoscroll *(new-surface acceptance)*

Press and hold inside the first paragraph, drag down past the bottom edge of the
editor and hold there so it autoscrolls, then release several blocks later.

- Observe: does the editor autoscroll while your finger is held at the edge?
- Observe: the footer's `… selected` count — does it grow smoothly and keep
  growing during autoscroll?
- Acceptance (new surface): selection extends continuously across block
  boundaries; autoscroll runs; the count matches the visible highlight.

### N2 — the anchor scrolls out of view *(new-surface acceptance)*

Start a drag-selection in the **first** block, then drag/autoscroll far enough
that the first block leaves the screen entirely, then release.

- Observe: is the selection still anchored at the original start? Read the
  footer — the selection start should be near 0 and the count large.
- Scroll back up: is the first block still highlighted?
- Acceptance (new surface): the anchor is a logical `Position(path, offset)` and
  survives its widget being destroyed. **This is the case RFC 024 §4.3 is built
  around.**

### N3 — select-all then copy yields complete exact source *(new-surface acceptance)*

1. `Scratch`, then type or paste a document containing **both** inline markup
   and a list, e.g.
   `alpha **bold** one` ⏎⏎ `- item one` ⏎ `- item two` ⏎⏎ `beta`
2. `Cmd/Ctrl+A` (row 8) or the context-menu **Select All** (phones).
3. Copy.
4. Paste into any plain-text field (Notes, a new `Scratch` in a second app —
   *not* back into flark) and compare with *Show source*.

**Measured v2 baseline** — do not be surprised by this, but do confirm it on
device and record the exact clipboard content:

```
PROBE-A source   = "alpha **bold** one\n\nbeta `code` two"
PROBE-A editable = "alpha bold one\n\nbeta code two"
PROBE-A clipboard= "alpha bold one\n\nbeta code two"
PROBE-A editables=1
PROBE-A modelSel = 0..35 of 35

PROBE-B editables=5
PROBE-B source   = "- one **b** item\n- two item\n\n> quoted line\n\ntail para"
PROBE-B editable = "one b item"
PROBE-B clipboard= "one b item"
PROBE-B modelSel = 0..53 of 53
```

**[measured:
`packages/flark_flutter/test/v2/flutter/flark_g1_selection_copy_baseline_probe_test.dart`]**

Read that as: on a plain-paragraph document v2 copies the whole document but
with **markers stripped** (`**bold**` → `bold`); on a structured document the
model selection is correctly the whole document (`0..53 of 53`) yet the clipboard
receives **only the focused block**, still marker-stripped. v2 has no
`CopySelectionTextIntent` override, so copy falls through to `EditableText`,
which can only see its own editable's projected text.

- v2 baseline: **expected `Fsrc`, twice over.** Record the literal clipboard
  string.
- Acceptance (new surface): the clipboard equals the document source byte for
  byte — including at 1 MB, which is beyond what this manual pass can reach. Do
  the phone-scale version here; the 1 MB version belongs to G4's harness.

### N4 — shift-click extension *(row 8 only)*

Click in paragraph 1, then shift-click in paragraph 3.

- Acceptance: selection extends from the first click to the shift-click, across
  the blocks between. Footer count matches.
- Then shift-click *above* the original point: the selection should invert, not
  collapse.

### N5 — double-click and triple-click *(row 8 only)*

In a paragraph containing `alpha **bold** one`:
- Double-click `bold` → the word only. Footer should read `4 selected` (display
  word), and the selection must not include the hidden `**` markers.
- Double-click a word next to a styled run → still just that word.
- Triple-click → the whole paragraph/block.

Widget-tier equivalents of the first two exist
(`packages/flark_flutter/test/v2/flutter/flark_selection_gesture_test.dart`);
triple-click does not.

### N6 — touch handles and magnifier *(phones)*

Long-press a word to select it, then drag each round handle.
- Do both handles appear? Does the magnifier loupe appear while dragging?
- Drag a handle **past a block boundary** — does the selection extend into the
  next block, or does it stop at the edge?
- Drag a handle **onto a hidden marker position** (the boundary of a bold run) —
  does the handle snap outside the marker, or land inside it (which would let a
  later edit split `**`)?
- Acceptance (new surface): handles cross block boundaries; the magnifier
  follows; endpoints never land inside a hidden marker.

### N7 — typing to replace a cross-block selection

Select from the middle of one paragraph to the middle of the next (drag or
handles), then type a single character `Z`.

- Expect: both partial paragraphs merge into one, with `Z` at the join.
- Then open *Show source* — the two blocks must have merged into one paragraph,
  with no orphaned `**` or `` ` `` left behind from either side.
- Repeat with the selection spanning a list item into a paragraph.
- v2 has pinned widget-tier coverage for this shape
  (`flark_cross_block_selection_test.dart`); this pass checks it under a real
  IME rather than a synthetic edit.

### N8 — IME composition while a selection exists elsewhere, during scroll *(Lane 1 — the flagged breaker)*

RFC 024 §7: *"The last is where it is most likely to break."* Run it carefully
and **film it**.

1. Use the tall document from the Part B setup.
2. Select a range in a block near the **top** (drag-select a few words; footer
   shows a `selected` count). Note the count.
3. Without dismissing that selection, scroll the editor **down** until the
   selected block is off screen.
4. Tap into a block near the bottom and begin composing — on a CJK row, start a
   romaji/jamo conversion and **leave it uncommitted**.
5. While still composing, scroll up so the *original selected block* comes back
   into view.
6. Then commit the composition.

Record, at each step, whether:
- the top selection survives (footer still shows the same `selected` count) —
  or is silently discarded;
- the composition survives the scroll (underline still present, candidate window
  still open) — or commits early / cancels;
- committing writes into the block you were composing in, or into the *selected*
  range (a selection-replacement that should not have happened);
- the source afterwards is exactly what you intended.

This is the case most likely to produce a source-level defect. If any of it
misbehaves, the recording is worth more than the notes.

### N9 — platform Actions bypassing source authority *(row 8 primarily)*

RFC 024 §7 records a standing hazard: `EditableText` ships Actions (undo, cut,
paste) that write straight to its own controller, bypassing source authority.

1. Type `alpha **bold** one` (use the **B** toolbar button for the bold).
2. Press `Cmd/Ctrl+Z` (the *platform* undo — **not** the app's footer Undo
   button). Then press the footer **Undo** button on a fresh repeat.
3. Compare the two: does platform undo produce the same document state as the
   app's undo, or a different one? Does the *source* stay valid after platform
   undo (open *Show source*)?
4. Repeat with cut (`Cmd/Ctrl+X`) over a styled run, and with paste
   (`Cmd/Ctrl+V`) into the middle of a styled run.
5. On phones: the shake-to-undo / three-finger-swipe undo gesture, and the
   context menu's Cut / Paste.

- Record any state where display and source disagree, or where the app's Undo
  button becomes unable to walk back.

### N10 — long-document sanity *(Lane 2)*

With the tall pasted document: scroll top to bottom quickly, then type one
character at the very top and one at the very bottom.

- Record: how long the `Parsing` badge stays amber after each keystroke; whether
  the editor visibly stutters; whether the caret readout stays correct.
- This is *not* the jank gate (G2 owns that, with
  `example/lib/perf_harness.dart` in profile mode). It is a smoke check that the
  correctness pass was not run on an unrepresentatively tiny document.

---

## 6. What a failure looks like

Use exactly these codes in the sheet. If two apply, record the **more severe**
one (they are listed most severe first) and describe the other in Notes.

| Code | Name | What you actually see |
| --- | --- | --- |
| `Fcrash` | Crash / hang | App closes, editor freezes, keyboard stops responding, or `Parsing` never returns to `Parsed`. Grab `flutter logs` output |
| `Fsrc` | **Source defect** | Display looks right, but *Show source* is wrong: markers in the wrong place (`**hello **`), unbalanced markers (`**hello`), a space on the wrong side of a delimiter, an extra or missing `⏎`. **Always a hard failure, even when the screen is perfect** |
| `Fdisp` | Display defect (persistent) | After committing: raw markers still visible (`**bold**`, `` `code` ``, `>` at the start of a quote), text missing, text doubled (`hellohello`), or a paragraph rendered as the wrong block type |
| `Fcomp` | Composition defect | Text ends up correct, but the IME was disturbed: the suggestion strip reset mid-word, the candidate window closed or jumped away from the caret, the composing underline vanished before you committed, or kana "fixed" itself early |
| `Fsel` | Selection / caret defect | Caret jumps somewhere unrelated after a commit; a selection silently disappears (footer flips from `k selected` back to `m chars`); a drag stops at a block boundary; a handle lands inside a hidden marker |
| `Fflash` | Transient flash | A `**` / `` ` `` / `~~` / `#` / `>` appears for roughly one frame and then disappears; final state is correct. Slow-motion screen recording is the only reliable way to catch this — if you suspect it, film it |
| `Fperf` | Lag / resync loop | Keyboard suggestion strip flickering continuously; typing stalls then catches up in a burst; the `Parsing` badge oscillating while you type steadily |
| `P` | Pass | Both check (a) and check (b) match, and nothing above was observed |
| `Blocked` | Could not run | No such device/keyboard available, app wedged, step impossible. Say why |
| `N/A` | Not applicable | Per §3.2 |

**Severity, for triage after the pass:** `Fcrash` > `Fsrc` > `Fdisp` > `Fcomp` >
`Fsel` > `Fperf` > `Fflash`.

**When anything other than `P`:**

1. Screenshot the editor **and** the *Show source* sheet, before moving the
   caret.
2. Screen-record a repeat attempt.
   Android: `adb shell screenrecord /sdcard/ime.mp4`.
   iOS: Control Centre recording. macOS: `Cmd+Shift+5`.
3. Write the exact keystroke / candidate sequence into Notes — not "typed a
   word", but the literal keys and which candidate you picked.
4. Android: capture `adb shell settings get secure default_input_method` and
   `adb logcat -s ImeTracker InputMethodManagerService` around the repro.
   iOS/macOS: Console.app filtered to the app process.
5. Paste the *Copy raw source* clipboard content into the sheet's Source column.

---

## 7. What can be automated, and what cannot

Automating everything in the left column would shrink the manual pass to the
right column — worth doing before this is ever run a second time.

### 7.1 Automatable now, at widget tier (no device)

The harness already exists: `LiveRenderSequence`
(`packages/flark_flutter/test/v2/flutter/support/live_render_sequence.dart`)
drives typing, Enter, backspace, arrows, selection **by source offset**, paste
and style toggles, and re-runs an export round-trip gate after every op;
`LiveRenderSequenceGestures` (`support/live_render_gestures.dart`) issues real
pointer streams for long-press, double-tap and drag-select at source positions.

| Case | Status | Note |
| --- | --- | --- |
| S1, S2, S4, S5, S6, S7, S8, S9 | **already automated** | `flark_ime_input_test.dart` drives real `TextEditingValue` sequences with composing regions over the test text-input channel. Every expectation in §4 tagged **[pinned]** comes from there |
| S11a–S11e | **already automated** | `example/test/widget_test.dart` fence tests |
| N5 double-click | **already automated** | `flark_selection_gesture_test.dart` |
| N7 cross-block replace | **already automated** | `flark_cross_block_selection_test.dart` |
| S3 autocorrect | automatable, not written | The autocorrect *shape* (replace-range then commit) is a `TextEditingValue` sequence like any other |
| N3 select-all + copy | automatable, **should be pinned** | The probe in `flark_g1_selection_copy_baseline_probe_test.dart` already does it; promote it from a probe to an assertion once the target behaviour is decided |
| N4 shift-click | automatable, not written | Needs a shift-modified tap helper on `LiveRenderSequenceGestures` |
| N5 triple-click | automatable, not written | Needs a triple-tap helper |
| N7 with an IME composition over the cross-block selection | automatable, not written | Combines two existing harnesses |
| N9 platform Actions bypass | automatable, not written | Invoke `UndoTextIntent` / `PasteTextIntent` through `Actions` and assert `controller.markdown`. This is a *contract* test and should exist regardless of the device pass |

### 7.2 Automatable at integration tier (emulator/simulator, no human)

There is **no `integration_test/` directory in this repo today** — this tier has
to be created. It is worth it: it removes the scroll and autoscroll cases from
the manual pass.

| Case | Why integration tier |
| --- | --- |
| N1 autoscroll during drag | Needs a real scrollable with real ticker-driven autoscroll |
| N2 anchor destroyed while off screen | Needs real viewport recycling |
| N10 long-document sanity | Needs a real frame pipeline |
| S11 under a *simulated* CJK IME | An emulator can host Gboard Japanese; scripted input is unreliable, so treat this as a smoke test only |

### 7.3 Genuinely needs a human with a physical device

These cannot be faked, because the thing under test is the platform IME's own
behaviour reacting to what flark sends it.

- **Every row's real keyboard** — S1–S11 on rows 1–8. The simulated suite proves
  flark's response to a *modelled* IME; it cannot prove Gboard or the iOS
  Japanese IME behaves the way the model assumes. This is the entire point of
  the gate.
- **The composition-survival observation itself** — "did the suggestion strip
  reset / did the candidate window close" is only observable by watching a real
  keyboard.
- **S10 voice dictation** — no scripted equivalent.
- **N6 touch handles and magnifier** — the magnifier is a platform overlay.
- **N8** — real IME + real scrolling + real selection simultaneously; the
  interaction *is* the test.
- **N9 on phones** — shake-to-undo and the platform context menu.

### 7.4 The recommended split

Automate §7.1's "not written" rows and stand up §7.2 **before the second run of
this matrix**. The manual pass then reduces to: S1–S11 on rows 1–8 (real
keyboards), S10 on row 9, and N6/N8/N9-phone — roughly 35 minutes per phone
instead of 90.

---

## Appendix A — the *Show source* patch (prerequisite)

Without this, check (b) is not executable: the example app has no source view,
no export and no autosave, and copy yields projected text (§5 N3). Applied to
`example/lib/main.dart`; verified with `flutter analyze lib` (`No issues
found!`) and `flutter test test/widget_test.dart` (39 passing).

**Hunk 1** — in `_CommandCluster.build`, immediately after the `Table` button's
closing `),` and before the `],` that ends the `children:` list:

```dart
            // --- G1 device-test affordance. See
            // docs/testing/ime_device_matrix_runbook.md, Appendix A. ---
            const _ClusterDivider(),
            _CommandButton(
              buttonKey: const ValueKey('flark-example-command-show-source'),
              tooltip: 'Show source',
              icon: Icons.data_object,
              onPressed: () => _showSourceSheet(context, controller),
            ),
```

**Hunk 2** — a new top-level function, placed just before
`class _ClusterDivider`:

```dart
/// G1 device-test affordance: shows the exact Markdown source behind the live
/// editor, with whitespace made visible, plus a raw-source copy button.
Future<void> _showSourceSheet(
  BuildContext context,
  FlarkFlutterController controller,
) {
  final source = controller.markdown;
  final visible = source.isEmpty
      ? '(empty document)'
      : source.replaceAll(' ', '·').replaceAll('\n', '⏎\n');
  return showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    builder: (sheetContext) => SafeArea(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(
              'Exact source · ${source.length} chars '
              '(· = space, ⏎ = newline)',
              style: const TextStyle(fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 8),
            ConstrainedBox(
              constraints: const BoxConstraints(maxHeight: 320),
              child: SingleChildScrollView(
                child: SelectableText(
                  visible,
                  style: const TextStyle(
                    fontFamily: 'monospace',
                    fontSize: 13,
                    height: 1.4,
                  ),
                ),
              ),
            ),
            const SizedBox(height: 12),
            FilledButton.icon(
              onPressed: () =>
                  Clipboard.setData(ClipboardData(text: source)),
              icon: const Icon(Icons.copy_rounded),
              label: const Text('Copy raw source'),
            ),
          ],
        ),
      ),
    ),
  );
}
```

No imports need adding — `main.dart` already imports
`package:flutter/services.dart` for `Clipboard`.

To drop the affordance afterwards: `git checkout -- example/lib/main.dart`.

---

## Appendix B — pointers

- Gate definition and the acceptance list: RFC 024 §6 (G1) and §7 —
  `docs/architecture/rfc/rfc_024_bounded_inframe_markdown_engine.md`
- The reopen trigger that keys off S11 rows 1–8:
  `docs/architecture/architecture_position_2026-07-12.md` §4 W1 and §5 Step 3
- Recognizer matrix to file findings against:
  `doc/architecture/live_edit_intent_pipeline.md`
- Simulated IME suite (add reproductions here):
  `packages/flark_flutter/test/v2/flutter/flark_ime_input_test.dart` —
  `_composeAndCommit` is the shape to copy
- Invariant the two end-checks come from:
  `docs/architecture/v2/inline_delimiter_validity_2026-07-10.md`
- Frame-timing harness (gate G2, **not** this pass):
  `example/lib/perf_harness.dart`
