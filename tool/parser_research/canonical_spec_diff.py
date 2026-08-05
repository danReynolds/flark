#!/usr/bin/env python3
"""Compare a cmark-compatible renderer to cmark fixtures semantically.

The upstream runner compares serialized HTML and therefore counts harmless
differences such as literal versus entity-escaped quotes, `align` versus CSS,
and an empty `tbody`. This probe canonicalizes those representation choices so
the remaining failures are more indicative of parser-semantic differences.
"""

from __future__ import annotations

import argparse
from collections import Counter
from html.parser import HTMLParser
from pathlib import Path
import re
import subprocess


EXAMPLE_MARKER = "`" * 32


class CanonicalHtml(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.tokens: list[tuple] = []
        self.stack: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        normalized: list[tuple[str, str]] = []
        for name, value in attrs:
            value = value or ""
            if name == "align":
                normalized.append(("style", f"text-align: {value}"))
            else:
                normalized.append((name, value))
        self.tokens.append(("start", tag, tuple(sorted(normalized))))
        if tag not in {"br", "hr", "img", "input"}:
            self.stack.append(tag)

    def handle_startendtag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        self.handle_starttag(tag, attrs)

    def handle_endtag(self, tag: str) -> None:
        self.tokens.append(("end", tag))
        if tag in self.stack:
            reverse_index = self.stack[::-1].index(tag)
            del self.stack[len(self.stack) - reverse_index - 1 :]

    def handle_data(self, data: str) -> None:
        # Formatting whitespace between HTML tags is not Markdown semantics.
        normalized = data if "pre" in self.stack else re.sub(r"\s+", " ", data)
        if normalized.strip():
            token = ("text", normalized)
            if self.tokens and self.tokens[-1][0] == "text":
                self.tokens[-1] = ("text", self.tokens[-1][1] + normalized)
            else:
                self.tokens.append(token)

    def handle_comment(self, data: str) -> None:
        self.tokens.append(("comment", data))


def canonicalize(html: str) -> list[tuple]:
    parser = CanonicalHtml()
    parser.feed(html)
    parser.close()
    tokens = parser.tokens
    # Some renderers include an empty tbody while others omit it.
    return [
        token
        for index, token in enumerate(tokens)
        if not (
            token == ("start", "tbody", ())
            and index + 1 < len(tokens)
            and tokens[index + 1] == ("end", "tbody")
        )
        and not (
            token == ("end", "tbody")
            and index > 0
            and tokens[index - 1] == ("start", "tbody", ())
        )
    ]


def load_tests(spec_path: Path) -> list[dict]:
    tests: list[dict] = []
    markdown: list[str] = []
    expected: list[str] = []
    extensions: list[str] = []
    section = ""
    state = "prose"
    example = 0
    for line in spec_path.read_text(encoding="utf-8").splitlines(keepends=True):
        stripped = line.strip()
        if stripped.startswith(f"{EXAMPLE_MARKER} example"):
            extensions = stripped[len(EXAMPLE_MARKER) + len(" example") :].split()
            state = "markdown"
        elif stripped == EXAMPLE_MARKER:
            example += 1
            if "disabled" not in extensions:
                tests.append(
                    {
                        "example": example,
                        "section": section,
                        "markdown": "".join(markdown).replace("→", "\t"),
                        "html": "".join(expected).replace("→", "\t"),
                        "extensions": extensions,
                    }
                )
            markdown = []
            expected = []
            extensions = []
            state = "prose"
        elif stripped == "." and state == "markdown":
            state = "html"
        elif state == "markdown":
            markdown.append(line)
        elif state == "html":
            expected.append(line)
        elif state == "prose" and re.match(r"^#+ ", line):
            section = re.sub(r"^#+ ", "", line).strip()
    return tests


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--renderer", type=Path, required=True)
    parser.add_argument("--spec", type=Path, required=True)
    parser.add_argument("--extensions", nargs="*", default=[])
    parser.add_argument("--show", type=int, default=5)
    args = parser.parse_args()

    failures: list[tuple[dict, list[tuple], list[tuple]]] = []
    tests = load_tests(args.spec)
    for test in tests:
        extensions = sorted(set(args.extensions + test["extensions"]))
        command = [str(args.renderer), "--unsafe"]
        for extension in extensions:
            command.extend(["-e", extension])
        result = subprocess.run(
            command,
            input=test["markdown"],
            text=True,
            capture_output=True,
            check=True,
        )
        expected = canonicalize(test["html"])
        actual = canonicalize(result.stdout)
        if expected != actual:
            failures.append((test, expected, actual))

    counts = Counter(test["section"] for test, _, _ in failures)
    print(f"tests={len(tests)} passed={len(tests) - len(failures)} failed={len(failures)}")
    for section, count in counts.most_common():
        print(f"failure_section count={count} name={section}")
    for test, expected, actual in failures[: args.show]:
        print(f"failure_example number={test['example']} section={test['section']}")
        print(repr(test["markdown"]))
        print(f"expected={expected}")
        print(f"actual={actual}")


if __name__ == "__main__":
    main()
