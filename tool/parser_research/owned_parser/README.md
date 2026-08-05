# Flark-owned parser trial

This is an independent, disposable parser-authority experiment. Its production
dependency graph deliberately contains no Markdown parser library. It is not a
package implementation and must not be shipped while its declared conformance
manifest is incomplete.

The normative profile and stop/go criteria live one directory above in
`OWNED_PARSER_SPEC_TRIAL.md` and `OWNED_PARSER_SPEC_PROFILE.json`.

Run the current certification lane with:

```sh
cargo test --release
```

