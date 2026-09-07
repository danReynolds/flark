#!/usr/bin/env python3
"""Run the package's component checks, without touching the live Flark preview."""
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess

package = Path(__file__).resolve().parent.parent
os.chdir(package)
build = package / "build"
build.mkdir(exist_ok=True)

def run(args, output=None, env=None):
    if output:
        with (build / output).open("w") as handle:
            subprocess.run(args, check=True, stdout=handle, env=env)
    else:
        subprocess.run(args, check=True, env=env)

toolchain = subprocess.check_output(["rustup", "show", "active-toolchain"], cwd=package / "native", text=True).split()[0]
rust_env = os.environ.copy()
for key, binary in [("RUSTC", "rustc"), ("RUSTDOC", "rustdoc")]:
    rust_env[key] = subprocess.check_output(["rustup", "which", binary, "--toolchain", toolchain], text=True).strip()
run(["rustup", "run", toolchain, "cargo", "fmt", "--manifest-path", "native/Cargo.toml", "--check"])
run(["rustup", "run", toolchain, "cargo", "test", "--release", "--locked", "--manifest-path", "native/Cargo.toml"], env=rust_env)
run(["python3", "tool/build_wasm.py"])
run(["dart", "analyze", "--fatal-infos"])
run(["dart", "test"])
run(["dart", "run", "tool/probe.dart"], "native.json")
run(["dart", "build", "cli", "--target", "bin/flark_tree_sitter_probe.dart", "--output", "build/aot"])
run([str(build / "aot/bundle/bin/flark_tree_sitter_probe")], "aot.json")
run(["dart", "compile", "js", "tool/probe.dart", "-O2", "-o", "build/probe.js"])
run(["node", "tool/run_probe.mjs", "build/probe.js"], "js.json")
run(["dart", "compile", "wasm", "tool/probe.dart", "-o", "build/probe.wasm"])
run(["node", "tool/run_probe.mjs", "build/probe.mjs"], "dart-wasm.json")
results = {name: json.loads((build / f"{name}.json").read_text()) for name in ["native", "aot", "js", "dart-wasm"]}
assert all(value == results["native"] for value in results.values()), "Transport mismatch"
run(["dart", "build", "cli", "--target", "bin/flark_tree_sitter_worker_probe.dart", "--output", "build/worker-aot"])
run([str(build / "worker-aot/bundle/bin/flark_tree_sitter_worker_probe")], "worker-native.json")
run(["dart", "compile", "js", "tool/browser_probe.dart", "-O2", "-o", "build/browser_probe.js"])
run(["dart", "compile", "wasm", "tool/browser_probe.dart", "-o", "build/browser_probe.wasm"])
wasm = (package / "lib/assets/wasm/flark_tree_sitter.wasm").read_bytes()
receipt = {"cases": len(results["native"]), "runtimes": list(results), "system": platform.platform(),
           "wasmBytes": len(wasm), "wasmSha256": hashlib.sha256(wasm).hexdigest()}
receipt["baseCommit"] = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
receipt["dirtyCheckout"] = bool(subprocess.check_output(["git", "status", "--porcelain"], text=True))
receipt["dart"] = subprocess.check_output(["dart", "--version"], text=True).strip()
receipt["node"] = subprocess.check_output(["node", "--version"], text=True).strip()
receipt["rust"] = subprocess.check_output([rust_env["RUSTC"], "--version"], text=True).strip()
sources = {}
for folder in ["lib", "hook", "native/src", "native/queries", "native/examples", "bin", "test", "tool"]:
    for path in sorted((package / folder).rglob("*")):
        if path.is_file() and path.suffix in [".dart", ".rs", ".py", ".mjs", ".scm"]:
            sources[str(path.relative_to(package))] = hashlib.sha256(path.read_bytes()).hexdigest()
for name in ["pubspec.yaml", "native/Cargo.toml", "native/Cargo.lock", "native/wasm_compat.h"]:
    sources[name] = hashlib.sha256((package / name).read_bytes()).hexdigest()
receipt["sourceHashes"] = sources
receipt["nativeWorker"] = json.loads((build / "worker-native.json").read_text())
(build / "verification.json").write_text(json.dumps(receipt, indent=2) + "\n")
print(json.dumps(receipt))
