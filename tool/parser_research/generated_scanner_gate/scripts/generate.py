#!/usr/bin/env python3
"""Generate and verify a storable DFA from the pinned Comrak scanner source."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile


GATE = Path(__file__).resolve().parents[1]
RESEARCH = GATE.parent
COMRAK = RESEARCH / "comrak_inline_fragment_gate" / "vendor" / "comrak"
SCANNER_SOURCE = COMRAK / "src" / "scanners.re"
FIXED_SCANNERS = COMRAK / "src" / "scanners.rs"
STRINGS_SOURCE = COMRAK / "src" / "strings.rs"
CTYPE_SOURCE = COMRAK / "src" / "ctype.rs"
TEMPLATE = GATE / "rules" / "atx.template.re"
GENERATED_RULE = GATE / "rules" / "atx.re"
GENERATED_RUST = GATE / "src" / "atx_generated.rs"
CURSOR_TEMPLATE = GATE / "rules" / "atx_cursor.template.re"
CURSOR_RULE = GATE / "rules" / "atx_cursor.re"
CURSOR_RUST = GATE / "src" / "atx_cursor_generated.rs"
FENCE_CURSOR_TEMPLATE = GATE / "rules" / "open_code_fence_cursor.template.re"
FENCE_CURSOR_RULE = GATE / "rules" / "open_code_fence_cursor.re"
FENCE_CURSOR_RUST = GATE / "src" / "open_code_fence_cursor_generated.rs"
ATX_TAIL_CURSOR_RUST = GATE / "src" / "atx_tail_cursor.rs"
FUSED_ATX_CURSOR_RUST = GATE / "src" / "atx_fused_cursor.rs"
CHOP_TRAILING_HASHES_DONOR = GATE / "donors" / "chop_trailing_hashes.rs"
PROVENANCE = GATE / "provenance.json"
EXPECTED_VERSION = "re2c 4.3.1"


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def run(*args: str) -> None:
    subprocess.run(args, check=True)


def extract_atx_rule(source: str) -> str:
    function = re.search(
        r"pub fn atx_heading_start\(.*?^/\*!re2c\n(?P<body>.*?)^\*/\n}",
        source,
        flags=re.MULTILINE | re.DOTALL,
    )
    if function is None:
        raise SystemExit("pinned scanners.re has no recognizable atx_heading_start")
    for line in function.group("body").splitlines():
        candidate = line.strip()
        if candidate.startswith("[#]{1,6}") and candidate.endswith("{"):
            return candidate[:-1].rstrip()
    raise SystemExit("pinned atx_heading_start has no recognizable accepting rule")


def extract_open_code_fence_rules(source: str) -> tuple[str, str]:
    function = re.search(
        r"pub fn open_code_fence\(.*?^/\*!re2c\n(?P<body>.*?)^\*/\n}",
        source,
        flags=re.MULTILINE | re.DOTALL,
    )
    if function is None:
        raise SystemExit("pinned scanners.re has no recognizable open_code_fence")
    rules: dict[str, str] = {}
    action = " { return Some(cursor); }"
    for line in function.group("body").splitlines():
        candidate = line.strip()
        if not candidate.endswith(action):
            continue
        rule = candidate[: -len(action)].rstrip()
        if rule.startswith("[`]{3,}"):
            rules["backtick"] = rule
        elif rule.startswith("[~]{3,}"):
            rules["tilde"] = rule
    if set(rules) != {"backtick", "tilde"}:
        raise SystemExit(
            "pinned open_code_fence must have exact backtick and tilde accepting rules"
        )
    return rules["backtick"], rules["tilde"]


def extract_exact_function(source: str, signature: str, label: str) -> bytes:
    function = re.search(
        rf"(?ms)^{signature}\n.*?^}}\n",
        source,
    )
    if function is None:
        raise SystemExit(f"pinned donor has no recognizable {label}")
    return function.group(0).encode()


def parse_ctype_whitespace(ctype_source: str) -> tuple[bytes, list[int]]:
    table = re.search(
        r"(?ms)^const CMARK_CTYPE_CLASS: \[u8; 256\] = \[\n.*?^\];\n",
        ctype_source,
    )
    if table is None:
        raise SystemExit("pinned ctype.rs has no recognizable CMARK_CTYPE_CLASS")
    body = table.group(0).split("[", 2)[2].rsplit("]", 1)[0]
    body = re.sub(r"/\*.*?\*/", "", body, flags=re.DOTALL)
    values = [int(value.strip()) for value in body.split(",") if value.strip()]
    if len(values) != 256:
        raise SystemExit("pinned CMARK_CTYPE_CLASS must contain 256 entries")
    whitespace = [index for index, value in enumerate(values) if value == 1]
    return table.group(0).encode(), whitespace


def parse_matches_u8_values(function: bytes, label: str) -> list[int]:
    matched = re.search(
        r"matches!\(ch,\s*(?P<values>[0-9\s|]+)\)", function.decode()
    )
    if matched is None:
        raise SystemExit(f"pinned donor has no recognizable {label} byte class")
    return sorted(int(value.strip()) for value in matched.group("values").split("|"))


def parse_local_u8_array(source: str, name: str) -> list[int]:
    constant = re.search(
        rf"const {name}: \[u8; [0-9]+\] = \[(?P<values>[0-9,\s]+)\];",
        source,
    )
    if constant is None:
        raise SystemExit(f"local correspondent has no recognizable {name}")
    return [
        int(value.strip())
        for value in constant.group("values").split(",")
        if value.strip()
    ]


def same_or_fail(path: Path, expected: bytes) -> None:
    if not path.exists() or path.read_bytes() != expected:
        raise SystemExit(f"generated artifact is stale: {path.relative_to(GATE)}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    parser.add_argument(
        "--re2rust", default=os.environ.get("RE2RUST", shutil.which("re2rust"))
    )
    args = parser.parse_args()
    if not args.re2rust:
        raise SystemExit("pass --re2rust or set RE2RUST to pinned re2rust 4.3.1")

    version = subprocess.run(
        [args.re2rust, "--version"], check=True, text=True, capture_output=True
    ).stdout.strip()
    if version != EXPECTED_VERSION:
        raise SystemExit(f"expected {EXPECTED_VERSION!r}, got {version!r}")

    source_bytes = SCANNER_SOURCE.read_bytes()
    fixed_bytes = FIXED_SCANNERS.read_bytes()
    strings_source = STRINGS_SOURCE.read_text()
    ctype_source = CTYPE_SOURCE.read_text()
    atx_tail_cursor_bytes = ATX_TAIL_CURSOR_RUST.read_bytes()
    fused_atx_cursor_bytes = FUSED_ATX_CURSOR_RUST.read_bytes()
    chop_trailing_hashes_source = extract_exact_function(
        strings_source,
        r"pub fn chop_trailing_hashes\(mut line: &str\) -> \(&str, bool\) \{",
        "strings::chop_trailing_hashes",
    )
    rtrim_slice_source = extract_exact_function(
        strings_source,
        r"pub fn rtrim_slice\(i: &str\) -> &str \{",
        "strings::rtrim_slice",
    )
    is_space_or_tab_source = extract_exact_function(
        strings_source,
        r"pub fn is_space_or_tab\(ch: u8\) -> bool \{",
        "strings::is_space_or_tab",
    )
    isspace_char_source = extract_exact_function(
        ctype_source,
        r"pub fn isspace_char\(ch: char\) -> bool \{",
        "ctype::isspace_char",
    )
    ctype_table_source, donor_trim_bytes = parse_ctype_whitespace(ctype_source)
    donor_close_separator_bytes = parse_matches_u8_values(
        is_space_or_tab_source, "strings::is_space_or_tab"
    )
    local_tail_source = atx_tail_cursor_bytes.decode()
    if parse_local_u8_array(
        local_tail_source, "PINNED_DONOR_TRIM_BYTES"
    ) != donor_trim_bytes:
        raise SystemExit(
            "ATX tail correspondent trim bytes drifted from pinned donor helpers"
        )
    if parse_local_u8_array(
        local_tail_source, "PINNED_DONOR_CLOSE_SEPARATOR_BYTES"
    ) != donor_close_separator_bytes:
        raise SystemExit(
            "ATX tail correspondent close separators drifted from pinned donor helper"
        )
    chop_dependency_bytes = b"".join(
        [
            rtrim_slice_source,
            is_space_or_tab_source,
            isspace_char_source,
            ctype_table_source,
        ]
    )
    rule = extract_atx_rule(source_bytes.decode())
    fence_backtick_rule, fence_tilde_rule = extract_open_code_fence_rules(
        source_bytes.decode()
    )
    rendered_rule = TEMPLATE.read_text().replace("@@ATX_RULE@@", rule).encode()
    rendered_cursor_rule = (
        CURSOR_TEMPLATE.read_text().replace("@@ATX_RULE@@", rule).encode()
    )
    rendered_fence_cursor_rule = (
        FENCE_CURSOR_TEMPLATE.read_text()
        .replace("@@BACKTICK_FENCE_RULE@@", fence_backtick_rule)
        .replace("@@TILDE_FENCE_RULE@@", fence_tilde_rule)
        .encode()
    )

    with tempfile.TemporaryDirectory(prefix="flark-generated-scanner-") as raw_tmp:
        tmp = Path(raw_tmp)
        fixed_out = tmp / "scanners.rs"
        run(
            args.re2rust,
            "--no-generation-date",
            "-o",
            str(fixed_out),
            str(SCANNER_SOURCE),
        )
        run("rustfmt", str(fixed_out))
        if fixed_out.read_bytes() != fixed_bytes:
            raise SystemExit(
                "pinned re2rust + rustfmt does not reproduce Comrak scanners.rs"
            )

        rule_in = tmp / "atx.re"
        rule_in.write_bytes(rendered_rule)
        storable_out = tmp / "atx_generated.rs"
        run(
            args.re2rust,
            "--no-generation-date",
            "--storable-state",
            "-o",
            str(storable_out),
            str(rule_in),
        )
        run("rustfmt", str(storable_out))
        storable_bytes = storable_out.read_bytes()

        cursor_rule_in = tmp / "atx_cursor.re"
        cursor_rule_in.write_bytes(rendered_cursor_rule)
        cursor_out = tmp / "atx_cursor_generated.rs"
        run(
            args.re2rust,
            "--no-generation-date",
            "--storable-state",
            "--no-unsafe",
            "-o",
            str(cursor_out),
            str(cursor_rule_in),
        )
        run("rustfmt", str(cursor_out))
        cursor_bytes = cursor_out.read_bytes()

        fence_cursor_rule_in = tmp / "open_code_fence_cursor.re"
        fence_cursor_rule_in.write_bytes(rendered_fence_cursor_rule)
        fence_cursor_out = tmp / "open_code_fence_cursor_generated.rs"
        run(
            args.re2rust,
            "--no-generation-date",
            "--storable-state",
            "--no-unsafe",
            "-o",
            str(fence_cursor_out),
            str(fence_cursor_rule_in),
        )
        run("rustfmt", str(fence_cursor_out))
        fence_cursor_bytes = fence_cursor_out.read_bytes()

    provenance = (
        json.dumps(
            {
                "comrak": "0.54.0",
                "fixed_scanners_sha256": digest(fixed_bytes),
                "fused_atx_line_scanner": {
                    "maintenance_class": "flark-owned-source-generic-one-pass-orchestrator",
                    "orchestrator_sha256": digest(fused_atx_cursor_bytes),
                },
                "generator": EXPECTED_VERSION,
                "scanner_source_sha256": digest(source_bytes),
                "selected_function": "atx_heading_start",
                "selected_rule": rule,
                "storable_scanner_sha256": digest(storable_bytes),
                "cursor_storable_scanner_sha256": digest(cursor_bytes),
                "fence_selected_function": "open_code_fence",
                "fence_selected_rules": [
                    fence_backtick_rule,
                    fence_tilde_rule,
                ],
                "fence_cursor_storable_scanner_sha256": digest(
                    fence_cursor_bytes
                ),
                "chop_trailing_hashes": {
                    "correspondent_sha256": digest(atx_tail_cursor_bytes),
                    "donor_close_separator_bytes": donor_close_separator_bytes,
                    "donor_dependencies_sha256": digest(chop_dependency_bytes),
                    "donor_function_sha256": digest(chop_trailing_hashes_source),
                    "donor_trim_bytes": donor_trim_bytes,
                    "maintenance_class": "flark-owned-handwritten-forward-correspondent",
                    "selected_function": "strings::chop_trailing_hashes",
                },
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    ).encode()

    if args.check:
        same_or_fail(GENERATED_RULE, rendered_rule)
        same_or_fail(GENERATED_RUST, storable_bytes)
        same_or_fail(CURSOR_RULE, rendered_cursor_rule)
        same_or_fail(CURSOR_RUST, cursor_bytes)
        same_or_fail(FENCE_CURSOR_RULE, rendered_fence_cursor_rule)
        same_or_fail(FENCE_CURSOR_RUST, fence_cursor_bytes)
        same_or_fail(CHOP_TRAILING_HASHES_DONOR, chop_trailing_hashes_source)
        same_or_fail(PROVENANCE, provenance)
        print("generated scanner and donor provenance exact")
        return

    GENERATED_RULE.write_bytes(rendered_rule)
    GENERATED_RUST.write_bytes(storable_bytes)
    CURSOR_RULE.write_bytes(rendered_cursor_rule)
    CURSOR_RUST.write_bytes(cursor_bytes)
    FENCE_CURSOR_RULE.write_bytes(rendered_fence_cursor_rule)
    FENCE_CURSOR_RUST.write_bytes(fence_cursor_bytes)
    CHOP_TRAILING_HASHES_DONOR.parent.mkdir(parents=True, exist_ok=True)
    CHOP_TRAILING_HASHES_DONOR.write_bytes(chop_trailing_hashes_source)
    PROVENANCE.write_bytes(provenance)
    print("generated scanner and donor artifacts updated")


if __name__ == "__main__":
    main()
