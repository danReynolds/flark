# Pixel 6a Android qualification development receipts

Status: development evidence, not a Tier B or M7 pass.

- Device: physical Pixel 6a, Android 16 / API 36, arm64.
- Artifact: Flutter profile APK using `flark_core`'s package build hook. No
  manually staged `jniLibs` library and no `FLARK_V4_LIBRARY_PATH` were used.
- Display: served 60 Hz during the retained timing runs.
- Command: `./scripts/v4_android.sh profile 25041JEGR04775`, with the shape,
  workload, byte count, sample count, and optional reopen count supplied by
  `FLARK_PROFILE_*` environment variables.

## Final comparable receipts

| Receipt | Open | Full certification | Editor p99 / max | Peak over warmed baseline | Retained after close |
| --- | ---: | ---: | ---: | ---: | ---: |
| `ordinary_1mib_inline_120_reopen4.json` | 34.1 ms | 7.28 s | 8.56 / 8.58 ms | 26.6 MiB | 21.0 MiB |
| `ordinary_10mib_inline_120_warmed.json` (before source-ownership fixes) | 249.8 ms | 100.05 s | 8.34 / 8.44 ms | 100.0 MiB | 58.0 MiB |
| `ordinary_10mib_inline_120_warmed_optimized.json` | 138.3 ms | 93.14 s | 9.17 / 9.37 ms | 95.4 MiB | 61.5 MiB |

Every row above recorded zero editor-attributed and zero unexplained
over-budget frames. The 10 MiB ownership fixes remove the ABI's complete UTF-8
clone and release the actor startup `String` as soon as the persistent source
owns the document. They cut controller open by 44.6% and the absolute process
peak by 13.3 MiB in the retained before/after runs. Cross-run warmed baselines
vary, so the memory conclusion uses each run's own baseline.

The four same-process reopen samples in the 1 MiB lifecycle receipt were
265.7, 269.2, 266.9, and 269.8 MiB after full parse/close. That is a bounded
high-water plateau in this short diagnostic, not evidence of per-cycle linear
growth. It also does not waive the retained-RSS gate.

## Threshold result

The provisional mobile frame envelope passes for the retained ordinary,
giant-line, and tiny-block timing runs: editor p99/max remained below 16 ms
with no editor-attributed or unexplained misses. The 10 MiB ordinary run also
painted no raw Markdown projection frames during inline typing.

Memory does not pass the provisional mobile envelope:

- 1 MiB peak is below the 48 MiB minimum peak allowance, but its 21.0 MiB
  retained delta exceeds the 8 MiB post-close limit.
- 10 MiB peak is 95.4 MiB over baseline versus a 60.0 MiB allowance, and its
  61.5 MiB retained delta exceeds the 8 MiB post-close limit.
- the retained `controllerOpenMs` is viewport-first controller construction,
  not the contract's exact first-paint measurement; it must not be relabelled
  as that gate.
- complete 10 MiB background certification still takes 93.14 seconds. It does
  not block typing once the viewport is available, but it is an explicit scale
  optimization target.

The pre-warmed shape receipts remain useful for frame attribution only:

- `giant_line_10mib_typing_120.json`: 12.85 ms p99, 12.96 ms max.
- `tiny_blocks_5mib_typing_120.json`: 7.99 ms p99/max; complete certification
  took about 4 minutes 12 seconds for roughly 1.3 million tiny blocks.
- `ordinary_1mib_semantic_120.json`: 9.72 ms p99, 10.42 ms max.
- `ordinary_1mib_inline_600_warmed.json`: 10.54 ms p99, 11.92 ms max.

Their older memory fields are not claim evidence because they predate the
warmed baseline and two-second post-close sampling contract.

## Physical interaction evidence and remaining qualification

The package-native APK passed the deterministic Android integration smoke for
open, live projection, literal `*`, Backspace, structural Return plus its
successor, undo, and the dogfood shell. Direct physical checks also covered
Gboard text and `*` input, touch scrolling without accidental selection,
long-press word selection with the adaptive Copy/Cut/Paste/Select All toolbar,
background/resume, and display-only read mode. Source remained synchronized
with zero input resyncs in those checks.

This is not full Android production qualification. Selection handles and a
magnifier are absent/unproven; TalkBack, real composition/autocorrect,
predictive text, dictation, long thermal sessions, the competitor-derived
Android document envelope, and release-floor/current-device coverage remain
open. iOS has no physical-device evidence yet. Windows is out of scope.
