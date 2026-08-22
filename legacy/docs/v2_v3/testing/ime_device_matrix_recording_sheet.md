# IME device matrix — recording sheet (G1)

Copy this file to `docs/testing/results/ime_matrix_<yyyy-mm-dd>_<device>.md` and
fill it in. One copy **per device**. Script:
[`ime_device_matrix_runbook.md`](ime_device_matrix_runbook.md).

Result codes — use only these (runbook §6, most severe first):
`Fcrash` · `Fsrc` · `Fdisp` · `Fcomp` · `Fsel` · `Fperf` · `Fflash` · `P` ·
`Blocked` · `N/A`

---

## Header — fill this in first

| Field | Value |
| --- | --- |
| Date |  |
| Runner |  |
| Device model |  |
| OS version |  |
| App build (`git rev-parse --short HEAD`) |  |
| *Show source* patch applied? (Appendix A) | yes / no |
| Debug or profile build |  |
| Rows covered on this device |  |
| Lanes attempted | 1 / 1+2 / 1+2+3 |
| Screen recordings saved to |  |

**Keyboards as configured** (one line each — name, version from the app store
listing or app info, and the settings toggles you set):

| Row | Keyboard | Version | Toggles |
| --- | --- | --- | --- |
| 1 | Gboard English |  | autocorrect ☐ predictive ☐ suggestion strip ☐ |
| 2 | Samsung Keyboard |  | predictive ☐ |
| 3 | Gboard Japanese |  | 12-key ☐ QWERTY romaji ☐ |
| 4 | Gboard Korean |  | 2-Bulsik ☐ |
| 5 | iOS English |  | autocorrect ☐ predictive ☐ smart punctuation ☐ |
| 6 | iOS Japanese |  | ローマ字 ☐ かな ☐ |
| 7 | iOS Korean |  | |
| 8 | macOS input source |  | Hiragana ☐ Pinyin ☐ |
| 9a/9b | Voice dictation |  | |

---

## Part A — v2 baseline (S1–S11)

**A wrong source is a failure even when the display is right.** Paste the *Show
source* content (with `·` and `⏎`) into the Source column whenever the result is
not `P`, and for S2, S6 and S8 **even when it is** `P` — those three are the ones
whose expected shape is subtle.

| Case | Row | Result | Source seen (verbatim, `·`/`⏎`) | Composition survived? | Notes / recording file |
| --- | --- | --- | --- | --- | --- |
| S1 predictive words | 1 |  |  | n/a |  |
| S1 | 2 |  |  | n/a |  |
| S1 | 5 |  |  | n/a |  |
| S2 predictive + bold armed | 1 |  |  | yes / no |  |
| S2 | 2 |  |  | yes / no |  |
| S2 | 5 |  |  | yes / no |  |
| S3 autocorrect | 1 |  |  | n/a |  |
| S3 | 2 |  |  | n/a |  |
| S3 | 5 |  |  | n/a |  |
| S4a Japanese, plain | 3 (romaji) |  |  | yes / no |  |
| S4a | 3 (12-key) |  |  | yes / no |  |
| S4a | 6 (romaji) |  |  | yes / no |  |
| S4a | 6 (kana) |  |  | yes / no |  |
| S4a | 8 |  |  | yes / no |  |
| S4b Japanese, in bold run | 3 (romaji) |  |  | yes / no |  |
| S4b | 3 (12-key) |  |  | yes / no |  |
| S4b | 6 (romaji) |  |  | yes / no |  |
| S4b | 6 (kana) |  |  | yes / no |  |
| S4b | 8 |  |  | yes / no |  |
| S5 Korean jamo | 4 |  |  | yes / no |  |
| S5 | 7 |  |  | yes / no |  |
| S6 styled trailing edge | 1 |  |  | yes / no |  |
| S6 | 2 |  |  | yes / no |  |
| S6 | 3 |  |  | yes / no |  |
| S6 | 4 |  |  | yes / no |  |
| S6 | 5 |  |  | yes / no |  |
| S6 | 6 |  |  | yes / no |  |
| S6 | 7 |  |  | yes / no |  |
| S6 | 8 |  |  | yes / no |  |
| S7 backspace mid-composition | 1 |  |  | yes / no |  |
| S7 | 2 |  |  | yes / no |  |
| S7 | 3 |  |  | yes / no |  |
| S7 | 4 |  |  | yes / no |  |
| S7 | 5 |  |  | yes / no |  |
| S7 | 6 |  |  | yes / no |  |
| S7 | 7 |  |  | yes / no |  |
| S7 | 8 |  |  | yes / no |  |
| S8 inline code span | 1 |  |  | yes / no |  |
| S8 | 2 |  |  | yes / no |  |
| S8 | 3 |  |  | yes / no |  |
| S8 | 4 |  |  | yes / no |  |
| S8 | 5 |  |  | yes / no |  |
| S8 | 6 |  |  | yes / no |  |
| S8 | 7 |  |  | yes / no |  |
| S8 | 8 |  |  | yes / no |  |
| S9 replace selection by composing | 1 |  |  | yes / no |  |
| S9 | 2 |  |  | yes / no |  |
| S9 | 3 |  |  | yes / no |  |
| S9 | 4 |  |  | yes / no |  |
| S9 | 5 |  |  | yes / no |  |
| S9 | 6 |  |  | yes / no |  |
| S9 | 7 |  |  | yes / no |  |
| S9 | 8 |  |  | yes / no |  |
| S10 voice, plain | 9a |  |  | n/a |  |
| S10 voice, plain | 9b |  |  | n/a |  |
| S10 voice, bold armed | 9a |  |  | n/a |  |
| S10 voice, bold armed | 9b |  |  | n/a |  |

### S11 — the reopen-trigger case (rows 1–8)

**This grid decides the architecture trigger.** A row counts as *S11 clean* only
if all five sub-cases are `P` on that row.

| Row | S11a | S11b | S11c | S11d | S11e | Row clean? | Notes / recording file |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 Gboard English |  |  |  |  |  |  |  |
| 2 Samsung |  |  |  |  |  |  |  |
| 3 Gboard JP (romaji) |  |  |  |  |  |  |  |
| 3 Gboard JP (12-key) |  |  |  |  |  |  |  |
| 4 Gboard KR |  |  |  |  |  |  |  |
| 5 iOS English |  |  |  |  |  |  |  |
| 6 iOS JP (romaji) |  |  |  |  |  |  |  |
| 6 iOS JP (kana) |  |  |  |  |  |  |  |
| 7 iOS KR |  |  |  |  |  |  |  |
| 8 macOS |  |  |  |  |  |  |  |

**Verdict — S11 across rows 1–8:** ☐ clean ☐ dirty ☐ incomplete (rows not run: ____)

> Clean → Phase-4-style sync-primary work is unblocked as far as this gate is
> concerned. Dirty → the *input* architecture reopens
> (`docs/architecture/architecture_position_2026-07-12.md` §5 Step 3), and the
> failing rows are the evidence. Incomplete is not clean.

---

## Part B — RFC 024 §7 acceptance baseline (N1–N10)

Results here are **measurements of v2 as a reference implementation**, not bug
reports. Record what happened; the "expected on v2" column says where a failure
is already anticipated.

| Case | Row | Expected on v2 | Result | What actually happened | Notes / recording file |
| --- | --- | --- | --- | --- | --- |
| N1 drag-select across blocks + autoscroll | 1 | unknown |  |  |  |
| N1 | 5 |  |  |  |  |
| N1 | 8 |  |  |  |  |
| N2 anchor scrolls out of view | 1 | unknown |  |  |  |
| N2 | 5 |  |  |  |  |
| N2 | 8 |  |  |  |  |
| N3 select-all + copy = exact source | 1 | **`Fsrc`** (markers stripped; structured doc copies one block only — measured) |  | clipboard content, verbatim: |  |
| N3 | 5 | **`Fsrc`** |  |  |  |
| N3 | 8 | **`Fsrc`** |  |  |  |
| N4 shift-click extension | 8 | unknown |  |  |  |
| N5 double-click | 8 | `P` (pinned at widget tier) |  |  |  |
| N5 triple-click | 8 | unknown |  |  |  |
| N6 touch handles + magnifier | 1 | unknown |  |  |  |
| N6 | 5 | unknown |  |  |  |
| N7 type over cross-block selection | 1 | `P` (pinned at widget tier) |  |  |  |
| N7 | 5 |  |  |  |  |
| N7 | 8 |  |  |  |  |
| **N8 IME composing + selection elsewhere + scroll** | 3 or 6 | **unknown — RFC 024 flags this as the likeliest breaker** |  |  |  |
| N8 | 4 or 7 |  |  |  |  |
| N8 | 8 |  |  |  |  |
| N9 platform undo / cut / paste vs source authority | 8 | hazard recorded in RFC 024 §7 |  |  |  |
| N9 (shake / context menu) | 1 |  |  |  |  |
| N9 | 5 |  |  |  |  |
| N10 long-document sanity | 1 | unknown |  |  |  |
| N10 | 5 |  |  |  |  |

### N8 step-by-step observation (fill one block per row you ran it on)

Row: ____

| Step | Observation |
| --- | --- |
| 2. selection made near top — footer `selected` count | |
| 3. after scrolling it off screen — count still shown? | yes / no |
| 4. composition started at the bottom — underline / candidate window present? | yes / no |
| 5. scrolled the selected block back into view — still highlighted? composition still live? | |
| 6. on commit — where did the text land? | into the composing block / into the selected range / elsewhere |
| Source afterwards (verbatim) | |
| Result code | |

---

## Summary — write this before you put the phones down

1. **Anything that produced `Fsrc` or `Fcrash`** (list case + row, one line each):

2. **S11 rows 1–8 verdict** (from the grid above), and if dirty, which sub-case
   failed first on which row:

3. **N8 outcome in one sentence** — did composing survive a selection existing
   elsewhere during a scroll?

4. **Anything you could not run, and why** (`Blocked` rows):

5. **Anything surprising that has no row in this sheet.** New failure modes are
   the most valuable thing here — write them down even if they do not fit.
