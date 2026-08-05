#!/usr/bin/env python3
"""Quantify the private MD4C functions pulled across candidate seams.

This is a deliberately conservative source audit, not a C parser. MD4C keeps
each `md_*` function at column zero with its opening brace in the signature
preamble, so brace matching is sufficient for this pinned revision. The gate
uses the resulting dependency closure as a maintenance-surface indicator.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path
import re


FUNCTION = re.compile(r"^(md_[A-Za-z0-9_]+)\s*\(")
CALL = re.compile(r"\b(md_[A-Za-z0-9_]+)\s*\(")


@dataclass(frozen=True)
class Function:
    name: str
    start: int
    end: int
    source: str

    @property
    def lines(self) -> int:
        return self.end - self.start + 1


def functions(path: Path) -> dict[str, Function]:
    lines = path.read_text(encoding="utf-8").splitlines()
    found: dict[str, Function] = {}
    index = 0
    while index < len(lines):
        match = FUNCTION.match(lines[index])
        if match is None:
            index += 1
            continue
        start = index
        opening = index
        while opening < len(lines) and "{" not in lines[opening]:
            opening += 1
        if opening == len(lines):
            raise RuntimeError(f"no body for {match.group(1)}")
        depth = 0
        end = opening
        while end < len(lines):
            depth += lines[end].count("{") - lines[end].count("}")
            if depth == 0:
                break
            end += 1
        body = "\n".join(lines[start : end + 1])
        found[match.group(1)] = Function(
            name=match.group(1), start=start + 1, end=end + 1, source=body
        )
        index = end + 1
    return found


def closure(
    all_functions: dict[str, Function],
    roots: set[str],
    facade_boundaries: set[str] = frozenset(),
) -> tuple[set[str], set[str]]:
    pending = list(roots)
    reached: set[str] = set()
    boundaries: set[str] = set()
    while pending:
        name = pending.pop()
        if name in facade_boundaries:
            boundaries.add(name)
            continue
        if name in reached or name not in all_functions:
            continue
        reached.add(name)
        pending.extend(set(CALL.findall(all_functions[name].source)) - reached)
    return reached, boundaries


def report(
    label: str,
    all_functions: dict[str, Function],
    roots: set[str],
    facade_boundaries: set[str] = frozenset(),
) -> set[str]:
    reached, boundaries = closure(all_functions, roots, facade_boundaries)
    selected = [all_functions[name] for name in reached]
    print(
        f"surface={label} functions={len(selected)} "
        f"function_loc={sum(function.lines for function in selected)} "
        f"CH={sum(function.source.count('CH(') for function in selected)} "
        f"STR={sum(function.source.count('STR(') for function in selected)} "
        f"boundaries={','.join(sorted(boundaries)) or '-'}"
    )
    print("members=" + ",".join(sorted(reached)))
    return reached


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--source", type=Path, default=Path("/tmp/flark-md4c-gate/src/md4c.c")
    )
    args = parser.parse_args()
    all_functions = functions(args.source)
    print(
        f"source={args.source} source_loc="
        f"{len(args.source.read_text(encoding='utf-8').splitlines())} "
        f"md_functions={len(all_functions)}"
    )

    block = report(
        "md4c_block_refs",
        all_functions,
        {
            "md_analyze_line",
            "md_process_line",
            "md_end_current_block",
            "md_build_ref_def_hashtable",
            "md_leave_child_containers",
        },
    )
    table_inline = report(
        "md4c_table_inline_boundary", all_functions, {"md_analyze_inlines"}
    )
    union = block | table_inline
    union_functions = [all_functions[name] for name in union]
    print(
        f"surface=md4c_block_plus_table_inline functions={len(union)} "
        f"function_loc={sum(function.lines for function in union_functions)} "
        f"CH={sum(function.source.count('CH(') for function in union_functions)} "
        f"STR={sum(function.source.count('STR(') for function in union_functions)}"
    )

    # The old version of this audit stopped at md_end_current_block. That was
    # not an honest lexical boundary: the Comrak facade does not finalize the
    # pending paragraph/setext block or remove consumed definition lines.
    # Keep finalization in the closure and stop only at the lexical recognizers.
    report(
        "md4c_orchestration_with_actual_table_ref_lexical_boundaries",
        all_functions,
        {
            "md_analyze_line",
            "md_process_line",
            "md_end_current_block",
            "md_leave_child_containers",
        },
        {"md_is_table_underline", "md_is_link_reference_definition"},
    )

    # Flark disables MD4C footnotes. Treating the dead-profile footnote
    # recognizer as a boundary shows the corresponding selected-profile
    # surface while retaining md_consume_link_reference_definitions itself.
    report(
        "md4c_selected_profile_table_ref_lexical_boundaries",
        all_functions,
        {
            "md_analyze_line",
            "md_process_line",
            "md_end_current_block",
            "md_leave_child_containers",
        },
        {
            "md_is_table_underline",
            "md_is_link_reference_definition",
            "md_is_footnote_definition",
        },
    )

    # The facade already exposes exact HTML start/end scanners too. This is
    # the equal-runtime-seam count for an ordinary-line hybrid; oversized HTML
    # still needs a separately proved resumable scanner path.
    report(
        "md4c_selected_profile_all_existing_lexical_boundaries",
        all_functions,
        {
            "md_analyze_line",
            "md_process_line",
            "md_end_current_block",
            "md_leave_child_containers",
        },
        {
            "md_is_table_underline",
            "md_is_link_reference_definition",
            "md_is_footnote_definition",
            "md_is_html_block_start_condition",
            "md_is_html_block_end_condition",
        },
    )


if __name__ == "__main__":
    main()
