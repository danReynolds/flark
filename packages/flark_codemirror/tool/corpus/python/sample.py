#!/usr/bin/env python3
"""A module docstring
spanning lines."""
from __future__ import annotations

import asyncio, os.path as osp
from dataclasses import dataclass, field
from typing import Iterable, Optional

GREETING = 'hello'
RAW = r'\d+\s*' + b"bytes" .decode()
PATH = rb'\\server\share'


@dataclass(frozen=True)
class Point:
    x: float = 0.0
    y: float = field(default=0.0)

    def __add__(self, other: "Point") -> Point:
        return Point(self.x + other.x,
                     self.y + other.y)

    @classmethod
    def origin(cls) -> "Point":
        return cls()


def numbers():
    return [0, 7, 0x1F, 0o17, 0b1010, 1_000_000, 3.14, .5, 1e-9, 2j, 10L]


async def fetch(urls: Iterable[str], *, limit: int = 3) -> list[str]:
    results = []
    async with asyncio.timeout(10):
        for url in urls:
            if (n := len(url)) > limit and not url.startswith("#"):
                results.append(f"{url!r} has {n:>4} chars and {{braces}}")
            elif url is None or url in ("", None):
                continue
            else:
                pass
    return results


def describe(value):
    match value:
        case {"kind": "point", "x": x, "y": y}:
            return f'point at {x}, {y}'
        case [first, *rest]:
            return "list starting " + str(first)
        case _:
            return None


class Stack(list):
    """A stack."""

    def push(self, item):
        self.append(item)

    def pop_or(self, default=None):
        try:
            return self.pop()
        except IndexError as error:
            print(error, file=sys.stderr)
            raise
        finally:
            del default


squares = {n: n ** 2 for n in range(10) if n % 2 == 0}
pairs = [(a, b)
         for a in range(3)
         for b in range(3)]
total = sum(
    value
    for value in squares.values()
)
handler = lambda event: event.get("type", "unknown")
long_line = 1 + \
    2
text = """triple
'quoted' "text" \""" still text
"""
nested = f"outer {f'inner {total}'} end"
global_counter = 0


def bump():
    global global_counter
    global_counter += 1
    def inner():
        nonlocal_value = 1
        return nonlocal_value
    return inner()


if __name__ == "__main__":
    while True:
        break
    assert bump() == 1, "bumped"
    print(osp.join("a", "b"), end="")
