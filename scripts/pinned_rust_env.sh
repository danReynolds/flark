#!/usr/bin/env bash

# Shared Rust toolchain selection for every first-party native entry point.
# Keep this value aligned with rust-toolchain.toml and CI. Selecting the exact
# rustup binaries also makes the gate deterministic when Homebrew's cargo comes
# earlier on PATH.
FLARK_RUST_TOOLCHAIN="1.98.0"

flark_use_pinned_rust() {
  local cargo_path rustc_path rust_bin actual
  if command -v rustup >/dev/null 2>&1; then
    cargo_path="$(rustup which cargo --toolchain "$FLARK_RUST_TOOLCHAIN")" || {
      echo "missing Rust toolchain $FLARK_RUST_TOOLCHAIN; run:" >&2
      echo "  rustup toolchain install $FLARK_RUST_TOOLCHAIN --profile minimal" >&2
      return 1
    }
    rustc_path="$(rustup which rustc --toolchain "$FLARK_RUST_TOOLCHAIN")" || return 1
    rust_bin="$(dirname "$cargo_path")"
    export PATH="$rust_bin:$PATH"
    export RUSTC="$rustc_path"
  fi

  actual="$(rustc --version)"
  case "$actual" in
    "rustc $FLARK_RUST_TOOLCHAIN "*) ;;
    *)
      echo "expected rustc $FLARK_RUST_TOOLCHAIN, found: $actual" >&2
      return 1
      ;;
  esac
}

flark_use_pinned_rust
