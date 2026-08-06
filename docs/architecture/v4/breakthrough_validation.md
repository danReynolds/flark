# Is there a breakthrough here? Mostly no. Recorded so it is not re-litigated.

**2026-08-06.** Three independent researchers tested the "flark is a Markdown
*document runtime*, not just an editor" thesis against the field. Three of four
claims died. Recorded with the evidence so nobody proposes them again.

## Killed

**1. "Best substrate for agent-edited Markdown."** Dead. `content.replace(old,
new)` is already byte-exact — no parser needed to splice a string without
reformatting. Incrementality buys nothing at agent edit *frequency*. And the
incumbents did not stumble into whole-file rewrite, they **chose** it: Cursor's
own engineering post cites model-side reasons (more reasoning tokens, models
have seen more whole files than diffs, models can't count line numbers) that no
substrate can change. Most damning — **Notion moved agents off its own block
model onto Markdown find-and-replace**, citing token density and hierarchical-JSON
round-trips. The company with the most to gain from structured agent editing
shipped string replace. Structure-aware Markdown editing does exist
(`markdown-patch`, generic MCP servers) and sits at 1–15 stars.

**2. "Streaming Markdown is unsolved."** Dead as a thesis. Six or more
incremental streaming parsers already exist across JS, Rust and Kotlin; one is
recommended by name in Google's docs; and one (`incremark`) already solves the
reference-definition-during-streaming case we would have held up as the
differentiator. The naive full-reparse norm is real and genuinely O(n²), but on
desktop it costs a couple of percent of CPU across a generation. The symptoms
practitioners actually complain about are flicker (fixed cheaply by source
mutation, e.g. `remend`) and freezes from regex backtracking and syntax
highlighting — not from Markdown parsing.

**3. "Collaborative Markdown over source is novel."** Dead. It is the
*mainstream* choice for Markdown-native tools, not an exotic one: Peerdraft and
Relay for Obsidian, HedgeDoc for a decade, Ink & Switch's own writing tool on
Automerge text + CodeMirror. We would be entering a market, not creating one.
(The round-trip *damage* argument against rich-text models remains correct and
well-evidenced — but "source-as-truth is therefore a better collaborative
foundation" does not follow, and Peritext refutes the strong form.)

**4. "Ship the engine as an independent artifact."** Dead economically. Exact
whole-file CommonMark parses at ~275 MiB/s. Our own measured corpora top out at
24–71 KB p99. That multiplies to **~0.25 ms for a full exact reparse of the
largest real document**. The territory is unoccupied because the return is
near-zero, and three sophisticated maintainers independently *chose* not to
enter it while documenting why. Also: the technique is not conceptually novel —
rust-analyzer/Salsa solves a strictly harder global-name-resolution problem
incrementally in production, and Typst/comemo does incremental markup reparse in
Rust with memo-stable spans.

## A correction we must make about our own claim

**We do not currently have "exact incremental CommonMark+GFM."** The 481/652
figure is structural admission plus semantic replay. The *incremental* path
covers paragraphs, blanks, code blocks, headings, thematic breaks and depth-one
tight lists — with block quotes, nested and loose lists, and tables failing
closed as `Unsupported`. Publishing the strong claim would be falsified by the
first person who types `> quote`.

This conflation has appeared in our own summaries. Stop making it.

## What survived — narrow, but real

1. **Keystroke-frequency concurrency with Markdown source as truth.** The one
   genuinely empty square. Everyone doing live human+AI co-editing does it over
   a rich-text CRDT with Markdown as a lossy wire format; everyone keeping
   Markdown as truth has no concurrency model beyond a lock. Moment's devlog is
   explicit — they **block AI writes while the user is editing** and merge
   afterwards.
2. **The giant unstable tail block.** Every block-caching streamer structurally
   degenerates on it, and it is *the* common LLM output shape (one long code
   fence). Fuel-bounded resumable parse is the correct answer.
3. **Editability during a stream.** The `remend`-style fix mutates the source,
   which forecloses this. We never rewrite the source, so we can offer it.
4. **Flutter.** None of the above exists there at all.

## Recommendation

**Position the editor, not the runtime.** Build the Flutter Markdown editor;
that is where the unserved ground actually is. Treat streaming and multi-source
editing as capabilities the architecture affords cheaply — not as the thesis.

Keep exactly one affordance for strategic reasons rather than hygiene: **make
the edit API source-agnostic with provenance from day one.** It is nearly free
now, impossible to retrofit, and it is the only thing that serves surviving
gap #1.

Harvest the novelty as **writing, not product**: one technical note on
incremental reference-definition resolution without suffix enumeration answers a
question Haverbeke, the tree-sitter-markdown maintainers and MacFarlane all
publicly declined — quotable, days of work, banks the credit without betting the
program on it.
