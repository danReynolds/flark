#!/usr/bin/env node

import {pathToFileURL} from "node:url"

const [lezerModule, shape = "giant-paragraph", rawBytes = "10485760"] = process.argv.slice(2)
if (!lezerModule) throw new Error("usage: lezer_liveness_probe.mjs LEZER_DIST_INDEX SHAPE BYTES")
const bytes = Number(rawBytes)
const {parser: baseParser, GFM} = await import(pathToFileURL(lezerModule))
const parser = baseParser.configure(GFM)

let source
if (shape == "giant-paragraph") {
  source = "a".repeat(bytes)
} else if (shape == "soft-lines") {
  source = "a\n".repeat(Math.floor(bytes / 2))
} else if (shape == "paragraphs") {
  source = "a\n\n".repeat(Math.floor(bytes / 3))
} else if (shape == "html-comment") {
  source = `<!--${"x".repeat(Math.max(0, bytes - 7))}-->`
} else if (shape == "table-row") {
  const body = "|x".repeat(Math.floor(Math.max(0, bytes - 1) / 2)) + "|"
  source = `${body}\n|-|\n${body}\n`
} else {
  throw new Error(`unknown shape ${shape}`)
}

const warm = source.slice(0, Math.min(source.length, 8192))
for (let i = 0; i < 5; i++) parser.parse(warm)

const started = process.hrtime.bigint()
const partial = parser.startParse(source)
const firstStarted = process.hrtime.bigint()
let tree = partial.advance()
const firstEnded = process.hrtime.bigint()
const parsedPosAfterFirstAdvance = partial.parsedPos
let calls = 1
while (!tree) {
  tree = partial.advance()
  calls++
}
const ended = process.hrtime.bigint()
const ms = (from, to) => Number(to - from) / 1e6
console.log(JSON.stringify({
  shape,
  sourceUnits: source.length,
  firstAdvanceMs: ms(firstStarted, firstEnded),
  parsedPosAfterFirstAdvance,
  advanceCalls: calls,
  totalMs: ms(started, ended),
  treeLength: tree.length,
}))
