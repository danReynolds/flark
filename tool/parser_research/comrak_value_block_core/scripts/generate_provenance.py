#!/usr/bin/env python3
"""Generate/check exact selected Comrak function-body provenance."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
DONOR = ROOT.parent / "comrak_inline_fragment_gate" / "vendor" / "comrak"
MANIFEST = ROOT / "provenance" / "comrak_0_54_block_functions.json"

BLOCK_METHODS = [
    "new", "parse", "process_line", "check_open_blocks",
    "check_open_blocks_inner", "find_first_nonspace",
    "parse_block_quote_prefix", "is_greentext", "parse_node_item_prefix",
    "parse_code_block_prefix", "parse_html_block_prefix",
    "fix_zero_end_columns", "open_new_blocks", "handle_blockquote",
    "detect_blockquote", "handle_atx_heading", "detect_atx_heading",
    "handle_code_fence", "detect_code_fence", "handle_html_block",
    "detect_html_block", "handle_setext_heading", "detect_setext_heading",
    "handle_thematic_break", "detect_thematic_break",
    "scan_thematic_break_inner", "handle_list", "detect_list",
    "handle_code_block", "detect_code_block", "handle_table",
    "detect_table", "advance_offset", "add_child", "add_text_to_container",
    "add_line", "finalize_document", "propagate_list_sourcepos", "finalize",
    "resolve_reference_link_definitions", "finalize_borrowed",
    "determine_list_tight", "parse_reference_inline",
]
BLOCK_FREE = ["parse_list_marker", "lists_match", "byte_matches"]
TABLE = [
    "try_opening_block", "try_opening_header", "try_opening_row", "row",
    "try_inserting_table_header_paragraph", "unescape_pipes",
    "adjust_table_counters", "get_num_autocompleted_cells", "matches",
]


def function_fragment(source: str, name: str) -> str:
    pattern = re.compile(rf"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?fn\s+{re.escape(name)}\b")
    matches = list(pattern.finditer(source))
    if len(matches) != 1:
        raise RuntimeError(f"expected one function {name}, found {len(matches)}")
    start = matches[0].start()
    brace = source.find("{", matches[0].end())
    if brace < 0:
        raise RuntimeError(f"function {name} has no body")

    depth = 0
    state = "code"
    index = brace
    while index < len(source):
        char = source[index]
        next_char = source[index + 1] if index + 1 < len(source) else ""
        if state == "line-comment":
            if char == "\n":
                state = "code"
        elif state == "block-comment":
            if char == "*" and next_char == "/":
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
        elif char == "/" and next_char == "/":
            state = "line-comment"
            index += 1
        elif char == "/" and next_char == "*":
            state = "block-comment"
            index += 1
        elif char == '"':
            state = "string"
        elif char == "'":
            # Rust lifetimes occur before identifiers. Treat only a quoted
            # scalar with a closing apostrophe as a character literal.
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
    raise RuntimeError(f"unterminated function {name}")


def existing_metadata() -> dict[tuple[str, str], dict[str, str]]:
    if not MANIFEST.exists():
        return {}
    data = json.loads(MANIFEST.read_text(encoding="utf-8"))
    return {
        (row["upstream_path"], row["upstream_name"]): row
        for row in data["functions"]
    }


def generate() -> dict[str, object]:
    previous = existing_metadata()
    groups = [
        ("src/parser/mod.rs", BLOCK_METHODS, "ValueBlockParser"),
        ("src/parser/mod.rs", BLOCK_FREE, "free"),
        ("src/parser/table.rs", TABLE, "table"),
    ]
    rows: list[dict[str, str]] = []
    for path, names, kind in groups:
        source = (DONOR / path).read_text(encoding="utf-8")
        for name in names:
            fragment = function_fragment(source, name)
            key = (path, name)
            old = previous.get(key, {})
            if kind == "ValueBlockParser":
                local = f"src/parser.rs::ValueBlockParser::{name}"
            elif kind == "table":
                local = f"src/table.rs::{name}"
            else:
                local = f"src/parser.rs::{name}"
            rows.append({
                "upstream_path": path,
                "upstream_name": name,
                "upstream_sha256": hashlib.sha256(fragment.encode()).hexdigest(),
                "local_correspondent": old.get("local_correspondent", local),
                "status": old.get("status", "pending"),
                "note": old.get("note", ""),
            })
    assert len(rows) == 55
    return {
        "schema_version": 1,
        "donor": {
            "repository": "https://github.com/kivikakk/comrak",
            "version": "0.54.0",
            "commit": "172c2ee7d2c5c262a28be3e407aadf705daea2b7",
        },
        "functions": rows,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    encoded = json.dumps(generate(), indent=2, sort_keys=False) + "\n"
    if args.check:
        if not MANIFEST.exists() or MANIFEST.read_text(encoding="utf-8") != encoded:
            print("provenance manifest is stale", file=sys.stderr)
            return 1
        print("provenance manifest exact: 55 functions")
        return 0
    print(encoded, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

