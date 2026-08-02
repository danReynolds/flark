# Flark v3 live-editor Web checkpoint

This is the earliest product-shaped checkpoint for the production-managed v3
path. It opens the real Web Worker + Wasm runtime, materializes one exact
parser-authored viewport page, and renders it through the virtualized Flutter
surface. The visible editor does not scan Markdown or display delimiter
markers.

From `example/`, launch it at the feedback URL with:

```sh
flutter run -d web-server --release \
  --web-hostname 127.0.0.1 \
  --web-port 8765 \
  -t lib/v3_live_editor_checkpoint.dart
```

Then open <http://127.0.0.1:8765/>.

## What to try

1. Confirm bold, emphasis, inline code, escaped punctuation, the hard line
   break, the heading, and the fenced-code body render without their Markdown
   delimiters. The escaped `*` should be visible while its canonical backslash
   remains hidden. The named `&copy;` and two-scalar `&ngE;` source tokens
   should appear only as their cooked `©` and `≧̸` text. The URI autolink
   containing `&amp;` should show and activate its cooked
   `https://e.test/?q=&` destination while export retains the exact source
   token. The sentence ending in `exact.` should wrap at the parser-certified
   hard break without showing its trailing spaces; canonical Markdown still
   retains those spaces and the exact physical line ending.
2. Click several different blocks. The single live input client should move to
   the selected block without a visual source-mode transition.
3. Type naturally in the active block and watch the `Parser current` status
   return after each revision.

This checkpoint intentionally uses a small complete document. Large-document
navigation uses the same surface through the implemented ordinal-window facade;
separate acceptance gates move one bounded editor among distant blocks while
keeping mounted presentations and the platform input client bounded. The
standalone 100,000-reference release-Worker gate in
`v3_engine_lab_web_runtime_test.dart` adds a rapid marker-free tail edit without
placing the reference prefix in `TextEditingController`. Its current Chrome
receipt is 4.2 ms maximum synchronous apply and 7.6 ms total across seven
zero-cadence edits. The combined
small-widget→100,000-reference-widget sequential reopen gate is also green
after correcting the Web module-loader cache lifetime; its current Chrome run
records a 5.1 ms maximum synchronous callback and 8.8 ms total callback time.
The character-reference examples are one narrow, parser-certified inline
vertical; they are not a claim of complete CommonMark or GFM coverage.
