# Link and image review

The later [resource/theme landing review](theming_merge_review_2026_09_08.md)
records fresh full-candidate checks and supersedes this initial local status.

Base: `46304ddd6522fa974376161a1af9072ecc900f28`, merged PR #42. The resource
changes are local on `codex/v5-links-images`; CI is skipped at the owner's
request. The implementation follows `links_images_plan.md`.

## Result and ownership

Comrak now supplies resolved destination/title strings for links, automatic
links and images in render-model schema v4. Raw source ranges remain intact.
Reference definitions, escaped URLs and entities use the same parser answer
on native and Wasm. The facade adds one resource value and four commands;
there is no host Markdown scanner or separate link resolver.

SetLink and SetImage preserve existing/selected inline Markdown unless the
user changes the visible label. Canonical serialization of explicit values is
checked against the candidate's parser-owned resource before publishing one
source/caret/history transaction. Cross-block, code, partial-owner and stale
edits reject atomically. Editing a reference use changes that occurrence to
inline Markdown and leaves the definition and other uses intact. Unlinking
retains formatting and avoids recreating automatic URLs. Explicit image removal
also clears its continuation intent while retaining surrounding formatting.

The Link dialog is available through the toolbar and Cmd/Ctrl-K. The Image
dialog supports URL, alt text and title; clicking a preview opens it, while
dragging across a preview remains scrolling. Cmd/Ctrl-click opens links; read-only
views use a normal click. Consumers own URL opening through a callback. The
workbench wires Flutter's url_launcher package to that callback.

Image previews reserve fixed 180-pixel slots beneath editable alt text.
Loading, success and failure have identical geometry. Only viewport images are
loaded/painted; the surface owns at most sixteen streams and requests decodes
bounded by 960 × 640. Flutter's shared image-cache policy is unchanged. Consumers
may supply a base URI, image provider, or disable previews. HTTP(S) is the
default provider; unsupported URLs and failures retain an unavailable slot.
Uploads, attachment storage, embeds and resizing are outside this slice.

## Review findings and corrections

Independent next-character testing caught image removal accidentally preserving
the removed image's pending closure inside bold text. The explicit removal path
now excludes that image while retaining outer formatting. Unlink tests exposed
the need to escape plain URL punctuation without stripping styled content.
Viewport review limited both loading and drawing, including many images in a
single projected row. Pointer activation waits for release and cancels after
movement so a touch scroll does not open an image dialog.

These are interaction families with independently specified intent, not rare
parser errors. Assertions cover source, caret, formatting, history, the next
input, first edited paint, delayed/failed image responses and stale dialogs.

## Local evidence

- Core and host analysis passed; 685 kernel and 832 Flutter host tests passed.
- Workbench analysis and 27 tests passed; 26 Chrome input, coloring-worker and
  font tests passed against the new parser Wasm.
- All 44 Rust tests passed, including conformance, fuzz and extraction checks.
  Native/Wasm models matched exactly on all 1,322 Markdown cases.
- Normal release Wasm and macOS profile builds passed. Diff checks passed.
- Hands-on release-browser authoring used Cmd-K, continued link text, and edited
  image URLs/alt text. A relative image loaded visibly; a missing URL showed a
  stable unavailable slot; Undo restored the image and typing continued below
  it. No browser warning/error logs were observed during those journeys.
  The refreshed final build also passed image-inside-bold removal followed by
  typing: the source became `**x**` and the removed image did not return.
  The owner's existing preview was refreshed with its 1,158-byte saved draft
  preserved. Raw local logs, source hashes and normal native binary hashes are
  under `receipts/resources-2026-09-08/`.
- The open-link callback is covered by mounted tests. The embedded browser's
  external-window result was not observable during the Open link canary, so it
  is not recorded as an external navigation pass.

## Native qualification boundary

The checkpoint frame harness built and started, then rejected
`AppLifecycleState.inactive` after the sixty-second foreground wait, before
measurement. An older ordinary app instance was closed after verifying its
17-byte draft was saved. No timing pass is inferred from capturing or raising
the profile window. The owner was asked for an awake foreground window; that
check remains pending. Foreground frame/workbench profiles, OS input/clipboard/
composition, native accessibility and ten process/background cycles remain
open. The normal native resource build is a build check only.
