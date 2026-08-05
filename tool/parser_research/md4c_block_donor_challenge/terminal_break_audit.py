#!/usr/bin/env python3
"""Prove the eight nested-list HTML differences are not inline softbreaks."""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import subprocess


EXAMPLE_MARKER = "`" * 32
EXAMPLES = {9, 272, 274, 287, 299, 300, 301, 303}


def load_examples(path: Path) -> dict[int, str]:
    markdown: list[str] = []
    examples: dict[int, str] = {}
    state = "prose"
    example = 0
    for line in path.read_text(encoding="utf-8").splitlines(keepends=True):
        stripped = line.strip()
        if stripped.startswith(f"{EXAMPLE_MARKER} example"):
            state = "markdown"
        elif stripped == EXAMPLE_MARKER:
            example += 1
            if example in EXAMPLES:
                examples[example] = "".join(markdown).replace("→", "\t")
            markdown = []
            state = "prose"
        elif stripped == "." and state == "markdown":
            state = "html"
        elif state == "markdown":
            markdown.append(line)
    return examples


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--spec", type=Path, required=True)
    parser.add_argument("--comrak", type=Path, required=True)
    args = parser.parse_args()

    examples = load_examples(args.spec)
    if set(examples) != EXAMPLES:
        raise RuntimeError(f"missing examples: {sorted(EXAMPLES - set(examples))}")

    for number in sorted(examples):
        result = subprocess.run(
            [str(args.comrak), "--to", "xml", "--sourcepos"],
            input=examples[number],
            text=True,
            capture_output=True,
            check=True,
        )
        softbreaks = result.stdout.count("<softbreak")
        paragraph_ranges = re.findall(r'<paragraph sourcepos="([^"]+)"', result.stdout)
        text_ranges = re.findall(r'<text sourcepos="([^"]+)"', result.stdout)
        print(
            f"example={number} softbreaks={softbreaks} "
            f"paragraphs={','.join(paragraph_ranges)} texts={','.join(text_ranges)}"
        )
        if softbreaks != 0:
            raise RuntimeError(f"example {number} unexpectedly contains a softbreak")
        if not paragraph_ranges or paragraph_ranges != text_ranges:
            raise RuntimeError(
                f"example {number} paragraph/text source positions diverged"
            )


if __name__ == "__main__":
    main()
