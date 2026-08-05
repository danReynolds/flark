#!/usr/bin/env python3
"""Replay and verify the isolated Comrak inline patch provenance."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[1]
PATCH = ROOT / "patches" / "comrak-inline-fragment-0.54.0.patch"
MANIFEST = ROOT / "provenance" / "comrak_inline_patch_0_54.json"
INLINE_PATCH_PATH = "src/parser/inline_fragment.rs"
INLINE_SOURCE = ROOT / "vendor" / "comrak" / INLINE_PATCH_PATH
TOUCHED_FUNCTIONS = [
    "new",
    "handle_newline",
    "handle_backticks",
    "handle_backslash",
    "handle_entity",
    "handle_pointy_brace",
    "handle_delim",
    "push_delimiter",
    "insert_emph",
    "push_bracket",
    "handle_close_bracket",
    "close_bracket_match",
]


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def braced_fragment(source: str, marker: str) -> str:
    start = source.index(marker)
    brace = source.index("{", start)
    depth = 0
    state = "code"
    index = brace
    while index < len(source):
        char = source[index]
        following = source[index + 1] if index + 1 < len(source) else ""
        if state == "line-comment":
            if char == "\n":
                state = "code"
        elif state == "block-comment":
            if char == "*" and following == "/":
                state = "code"
                index += 1
        elif state == "string":
            if char == "\\":
                index += 1
            elif char == '"':
                state = "code"
        elif state == "char":
            if char == "\\":
                index += 1
            elif char == "'":
                state = "code"
        elif char == "/" and following == "/":
            state = "line-comment"
            index += 1
        elif char == "/" and following == "*":
            state = "block-comment"
            index += 1
        elif char == '"':
            state = "string"
        elif char == "'":
            # Rust lifetimes are not character literals.
            close = source.find("'", index + 1, min(index + 8, len(source)))
            if close >= 0:
                state = "char"
        elif char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return source[start : index + 1]
        index += 1
    raise RuntimeError(f"unterminated braced fragment after {marker!r}")


def function_fragment(subject_impl: str, name: str) -> str:
    pattern = re.compile(
        rf"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?fn\s+{re.escape(name)}\b"
    )
    matches = list(pattern.finditer(subject_impl))
    if len(matches) != 1:
        raise RuntimeError(f"expected one Subject::{name}, found {len(matches)}")
    return braced_fragment(subject_impl, matches[0].group(0))


def patch_stats(patch: str) -> list[dict[str, object]]:
    rows: list[dict[str, object]] = []
    current: dict[str, object] | None = None
    for line in patch.splitlines():
        if line.startswith("diff --git "):
            path = line.split()[-1]
            if not path.startswith("b/"):
                raise RuntimeError(f"non-canonical patch destination: {line}")
            current = {"path": path[2:], "additions": 0, "deletions": 0}
            rows.append(current)
        elif current is not None and line.startswith("+") and not line.startswith("+++"):
            current["additions"] = int(current["additions"]) + 1
        elif current is not None and line.startswith("-") and not line.startswith("---"):
            current["deletions"] = int(current["deletions"]) + 1
    return rows


def refresh_inline_patch() -> None:
    """Replace only the isolated new-file section from the shared checkout."""
    patch = PATCH.read_text(encoding="utf-8")
    start_marker = f"diff --git a/{INLINE_PATCH_PATH} b/{INLINE_PATCH_PATH}\n"
    end_marker = "diff --git a/src/parser/inlines.rs b/src/parser/inlines.rs\n"
    start = patch.index(start_marker)
    end = patch.index(end_marker, start)
    source = INLINE_SOURCE.read_text(encoding="utf-8")
    if not source.endswith("\n"):
        raise RuntimeError(f"{INLINE_SOURCE} must end with a newline")
    lines = source.splitlines()
    blob = subprocess.run(
        ["git", "hash-object", str(INLINE_SOURCE)],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    section = "".join(
        [
            start_marker,
            "new file mode 100644\n",
            f"index 0000000..{blob[:7]}\n",
            "--- /dev/null\n",
            f"+++ b/{INLINE_PATCH_PATH}\n",
            f"@@ -0,0 +1,{len(lines)} @@\n",
            *(f"+{line}\n" for line in lines),
        ]
    )
    PATCH.write_text(patch[:start] + section + patch[end:], encoding="utf-8")


def generate(pristine: Path) -> dict[str, object]:
    vcs = json.loads((pristine / ".cargo_vcs_info.json").read_text(encoding="utf-8"))
    if vcs["git"]["sha1"] != "172c2ee7d2c5c262a28be3e407aadf705daea2b7":
        raise RuntimeError("pristine Comrak checkout is not the pinned 0.54.0 crate")

    patch = PATCH.read_text(encoding="utf-8")
    stats = patch_stats(patch)
    with tempfile.TemporaryDirectory(prefix="flark-inline-inventory.") as temporary:
        patched = Path(temporary) / "comrak"
        shutil.copytree(pristine, patched)
        subprocess.run(
            ["git", "apply", "--check", str(PATCH)], cwd=patched, check=True
        )
        subprocess.run(["git", "apply", str(PATCH)], cwd=patched, check=True)

        for row in stats:
            path = str(row["path"])
            base = pristine / path
            result = patched / path
            row["pristine_sha256"] = sha256_file(base) if base.exists() else None
            row["patched_sha256"] = sha256_file(result)

        base_source = (pristine / "src/parser/inlines.rs").read_text(encoding="utf-8")
        patched_source = (patched / "src/parser/inlines.rs").read_text(encoding="utf-8")
        impl_marker = "impl<'a, 'r, 'o, 'd, 'c, 'p> Subject"
        base_impl = braced_fragment(base_source, impl_marker)
        patched_impl = braced_fragment(patched_source, impl_marker)
        functions = []
        for name in TOUCHED_FUNCTIONS:
            base = function_fragment(base_impl, name)
            result = function_fragment(patched_impl, name)
            functions.append(
                {
                    "upstream_path": "src/parser/inlines.rs",
                    "upstream_name": name,
                    "pristine_sha256": sha256_bytes(base.encode()),
                    "patched_sha256": sha256_bytes(result.encode()),
                }
            )

    return {
        "schema_version": 1,
        "donor": {
            "crate": "comrak",
            "version": "0.54.0",
            "commit": vcs["git"]["sha1"],
        },
        "patch": {
            "path": "patches/comrak-inline-fragment-0.54.0.patch",
            "sha256": sha256_file(PATCH),
            "files": stats,
            "total_additions": sum(int(row["additions"]) for row in stats),
            "total_deletions": sum(int(row["deletions"]) for row in stats),
        },
        "sensitive_functions": functions,
    }


def default_pristine() -> Path:
    roots = sorted(
        Path.home().glob(".cargo/registry/src/*/comrak-0.54.0/.cargo_vcs_info.json")
    )
    if len(roots) != 1:
        raise RuntimeError(
            "pass --pristine; expected exactly one cached comrak-0.54.0 source"
        )
    return roots[0].parent


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--pristine", type=Path)
    parser.add_argument("--print", action="store_true", dest="print_manifest")
    parser.add_argument(
        "--refresh",
        action="store_true",
        help="refresh the isolated inline new-file section and manifest",
    )
    args = parser.parse_args()
    if args.refresh:
        refresh_inline_patch()
    generated = generate(args.pristine or default_pristine())
    encoded = json.dumps(generated, indent=2) + "\n"
    if args.refresh:
        MANIFEST.write_text(encoded, encoding="utf-8")
        print(
            "refreshed inline patch inventory: "
            f"{len(generated['patch']['files'])} files, "
            f"{generated['patch']['total_additions']} additions, "
            f"{generated['patch']['total_deletions']} deletions"
        )
        return 0
    if args.print_manifest:
        print(encoded, end="")
        return 0
    if not MANIFEST.exists() or MANIFEST.read_text(encoding="utf-8") != encoded:
        print("inline patch inventory is stale", file=sys.stderr)
        return 1
    print(
        "inline patch inventory exact: "
        f"{len(generated['patch']['files'])} files, "
        f"{len(generated['sensitive_functions'])} sensitive functions"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
