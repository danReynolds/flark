# Flark

`flark` is the Flutter product surface. Its headless Dart API lives in
`flark_core`; parser and source authority live in the Rust runtime behind the
fixed v4 ABI.

The current macOS vertical slice uses a custom `RenderBox`, Flutter delta text
input, a bounded 16 Ki UTF-16 input window, optimistic next-frame painting, and
certified incremental viewport rows.

Build and run from the repository worktree:

```sh
cargo build --manifest-path native/comrak_bridge/Cargo.toml --package flark-abi
cd packages/flark/example
FLARK_V4_LIBRARY_PATH="$PWD/../../../native/comrak_bridge/target/debug/libflark_abi.dylib" flutter run -d macos
```
