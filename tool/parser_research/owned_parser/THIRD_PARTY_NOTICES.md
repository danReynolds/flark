# Third-party notices for test inputs

The CommonMark specification and embedded examples are Copyright John
MacFarlane and licensed under CC-BY-SA 4.0. The pinned profile uses CommonMark
0.31.2 from <https://github.com/commonmark/commonmark-spec/tree/0.31.2>.

The GitHub Flavored Markdown specification and embedded examples are licensed
under CC-BY-SA 4.0. The pinned profile uses the specification sources from
<https://github.com/github/cmark-gfm> at commit
`499789b49373bfa045d0e7547e5ee63444c77bca`.

The parser implementation in this directory is Flark-owned. If future modules
adapt implementation algorithms from BSD/MIT sources, record the file-level
provenance and required notices here before landing them.

`src/parser.rs`'s code-span delimiter-run index adapts the pathological-
complexity strategy used by cmark's `src/inlines.c` backtick cache. cmark is
Copyright John MacFarlane and distributed under the BSD 2-Clause License:
<https://github.com/commonmark/cmark/blob/0.31.2/COPYING>.
