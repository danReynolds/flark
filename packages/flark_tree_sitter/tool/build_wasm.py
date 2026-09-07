#!/usr/bin/env python3
"""Build the same pinned crate for browsers, without patching grammar sources."""
import json
import os
from pathlib import Path
import shutil
import subprocess

package = Path(__file__).resolve().parent.parent
crate = package / "native"
env = os.environ.copy()
rustup = shutil.which("rustup")
if rustup:
    toolchain = subprocess.check_output([rustup, "show", "active-toolchain"], cwd=crate, text=True).split()[0]
    cargo = [rustup, "run", toolchain, "cargo"]
    env["RUSTC"] = subprocess.check_output([rustup, "which", "rustc", "--toolchain", toolchain], text=True).strip()
else:
    cargo = ["cargo"]
metadata = json.loads(subprocess.check_output(cargo + ["metadata", "--locked", "--format-version", "1"], cwd=crate, env=env))
language = next(p for p in metadata["packages"] if p["name"] == "tree-sitter-language")
headers = Path(language["manifest_path"]).parent / "wasm/include"
if not headers.is_dir():
    raise SystemExit("The pinned tree-sitter-language crate must provide Wasm C headers.")
# Published grammar crates predate the shared-header build metadata. Pass the
# upstream headers to their existing cc-rs builds; no vendored parser fork.
compat = crate / "wasm_compat.h"
env["CFLAGS_wasm32_unknown_unknown"] = f'-I"{headers}" -include "{compat}" ' + env.get("CFLAGS_wasm32_unknown_unknown", "")
env["CC_SHELL_ESCAPED_FLAGS"] = "1"
compiler = env.get("CC_wasm32_unknown_unknown")
if not compiler:
    candidates = [shutil.which("clang"), "/opt/homebrew/opt/llvm/bin/clang", "/usr/local/opt/llvm/bin/clang"]
    for candidate in candidates:
        if not candidate or not Path(candidate).is_file():
            continue
        targets = subprocess.run([candidate, "--print-targets"], capture_output=True, text=True)
        if "wasm32" in targets.stdout:
            compiler = candidate
            break
if not compiler:
    raise SystemExit("Set CC_wasm32_unknown_unknown to an LLVM clang with the wasm32 target.")
env["CC_wasm32_unknown_unknown"] = compiler
archiver = Path(compiler).parent / "llvm-ar"
if archiver.is_file():
    env.setdefault("AR_wasm32_unknown_unknown", str(archiver))
subprocess.run(cargo + ["build", "--release", "--locked", "--lib", "--target", "wasm32-unknown-unknown"], cwd=crate, env=env, check=True)
output = package / "lib/assets/wasm/flark_tree_sitter.wasm"
output.parent.mkdir(parents=True, exist_ok=True)
shutil.copyfile(Path(metadata["target_directory"]) / "wasm32-unknown-unknown/release/flark_tree_sitter.wasm", output)
print(output)
