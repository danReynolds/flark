# Snippet ABI 4

The six exported functions and allocation ownership are defined in
`native/src/abi.rs`. Version 4 adds detection and extends the language catalog. Native and
Wasm assets with older versions fail initialization; rebuild both assets together.
The Dart public range and edit types retain their meaning.

Analysis input is exact UTF-8 source plus the numeric language. Output is UTF-8
JSON with `version`, `language`, `status`, `scope_sets` and `spans`:

```json
{"version":4,"language":1,"status":"highlighted","scope_sets":[["keyword"],[]],"spans":[[3,3,0],[4,4,1]]}
```

Each span is `[endUtf16, endByte, scopeSetIndex]`. Its start is the previous
span's end, or zero for the first span. Scope sets retain their original order;
the response interns each set rather than repeating scope strings per token.
Dart checks tuple size, indices, types, nonempty scope names, increasing ranges,
full source coverage, surrogate boundaries and UTF-8/UTF-16 agreement. Public
spans include both starts and ends and expose immutable, shared scope lists.

The edit request and response retain the ABI 2 JSON shape, with `version: 4` in
the response. The caller still applies one proposed replacement to the exact
source that generated it. No document IDs, parser tree handles or mutable
selection state cross this ABI.

Detection uses the analysis buffer ABI, with its unused language argument set to
zero. Input is at most 128 UTF-16 units; output is
`{"version":4,"language":5}` for Ruby, for example. Dart validates the shape,
version and catalog bounds before caching a result. Invalid UTF-8/JSON is an
engine error, never an inferred language.

Language IDs are append-only: 0 plain, 1 Dart, 2 JavaScript, 3 Python, 4 YAML,
5 Ruby, 6 TypeScript, 7 Rust, 8 Go, 9 JSON, 10 CSS, 11 Bash, 12 HTML, 13 XML,
14 SQL. Detection returns zero without positive query evidence. No mutable
language guess or document handle crosses the ABI.
