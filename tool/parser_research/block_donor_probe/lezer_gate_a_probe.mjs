#!/usr/bin/env node

// Structural differential only. This does not render Markdown or implement a
// Markdown recognizer. It projects block nodes from Lezer and Comrak into the
// same small vocabulary, then compares the projections on Gate A fixtures.

import {readFileSync} from "node:fs"
import {spawnSync} from "node:child_process"
import {pathToFileURL} from "node:url"
import {dirname, resolve} from "node:path"

const [fixtureRoot, lezerModule, comrakBin, detailFlag] = process.argv.slice(2)
if (!fixtureRoot || !lezerModule || !comrakBin) {
  throw new Error("usage: lezer_gate_a_probe.mjs FIXTURE_ROOT LEZER_DIST_INDEX COMRAK_BIN")
}

const {parser: baseParser, GFM} = await import(pathToFileURL(lezerModule))
const {TreeFragment} = await import(pathToFileURL(resolve(dirname(dirname(lezerModule)), "node_modules/@lezer/common/dist/index.js")))
const lezer = baseParser.configure(GFM)

const commonmarkSections = new Set([
  "Tabs",
  "Setext headings",
  "HTML blocks",
  "Block quotes",
  "List items",
  "Lists",
])
const gfmSections = new Set(["Tables (extension)"])

const commonmark = JSON.parse(readFileSync(`${fixtureRoot}/common_mark_tests.json`, "utf8"))
  .filter(fixture => commonmarkSections.has(fixture.section))
  .map(fixture => ({...fixture, profile: "commonmark"}))
const gfm = JSON.parse(readFileSync(`${fixtureRoot}/gfm_tests.json`, "utf8"))
  .filter(fixture => gfmSections.has(fixture.section))
  .map(fixture => ({...fixture, profile: "gfm"}))
const fixtures = commonmark.concat(gfm)

function mappedLezerType(name) {
  if (name == "Document") return "document"
  // Comrak's XML projection doesn't expose the fenced bit. Gate A's structural
  // comparison therefore treats both exact code-block forms as one block type.
  if (name == "CodeBlock" || name == "FencedCode") return "code_block"
  if (name == "Blockquote") return "block_quote"
  if (name == "HorizontalRule") return "thematic_break"
  if (name == "BulletList") return "list:bullet"
  if (name == "OrderedList") return "list:ordered"
  if (name == "ListItem") return "item"
  if (name == "Paragraph") return "paragraph"
  if (name == "HTMLBlock" || name == "CommentBlock" || name == "ProcessingInstructionBlock") return "html_block"
  if (name == "Table") return "table"
  // Comrak's XML projection doesn't expose which row is the header.
  if (name == "TableHeader" || name == "TableRow") return "table_row"
  if (name == "TableCell") return "table_cell"
  let atx = /^ATXHeading([1-6])$/.exec(name)
  if (atx) return `heading:${atx[1]}`
  let setext = /^SetextHeading([12])$/.exec(name)
  if (setext) return `heading:${setext[1]}`
  return null
}

function projectLezer(node) {
  const children = []
  for (let child = node.firstChild; child; child = child.nextSibling) {
    children.push(...projectLezer(child))
  }
  const type = mappedLezerType(node.name)
  return type ? [{type, children}] : children
}

function attrs(text) {
  const result = Object.create(null)
  for (const match of text.matchAll(/([\w:-]+)="([^"]*)"/g)) result[match[1]] = match[2]
  return result
}

function mappedComrakType(name, attributes) {
  if (name == "document") return "document"
  if (name == "code_block") return "code_block"
  if (name == "block_quote") return "block_quote"
  if (name == "thematic_break") return "thematic_break"
  if (name == "list") return `list:${attributes.type}`
  if (name == "item") return "item"
  if (name == "paragraph") return "paragraph"
  if (name == "html_block") return "html_block"
  if (name == "heading") return `heading:${attributes.level}`
  if (name == "table") return "table"
  if (name == "table_row") return "table_row"
  if (name == "table_cell") return "table_cell"
  return null
}

function projectComrak(xml) {
  const root = {type: "_root", children: []}
  const stack = [root]
  for (const match of xml.matchAll(/<\s*(\/?)\s*([\w:-]+)([^>]*)>/g)) {
    const closing = match[1] == "/"
    const name = match[2]
    if (name.startsWith("?") || name.startsWith("!")) continue
    if (closing) {
      const index = stack.map(node => node.xmlName).lastIndexOf(name)
      if (index >= 1) stack.length = index
      continue
    }
    const attributeText = match[3]
    const type = mappedComrakType(name, attrs(attributeText))
    const parent = stack[stack.length - 1]
    const node = {xmlName: name, type, children: []}
    if (type) parent.children.push(node)
    const selfClosing = /\/\s*$/.test(attributeText)
    if (!selfClosing) stack.push(type ? node : {...node, children: parent.children})
  }
  function clean(node) {
    return {type: node.type, children: node.children.map(clean)}
  }
  return root.children.map(clean)
}

function signature(nodes) {
  return JSON.stringify(nodes)
}

function exactTreeSignature(tree) {
  const rows = []
  const cursor = tree.cursor()
  for (;;) {
    rows.push([cursor.name, cursor.from, cursor.to])
    if (!cursor.next()) break
  }
  return JSON.stringify(rows)
}

function renderComrak(markdown, profile) {
  const args = ["--unsafe", "--to", "xml"]
  if (profile == "gfm") args.push("--extension", "table")
  const result = spawnSync(comrakBin, args, {input: markdown, encoding: "utf8", maxBuffer: 16 * 1024 * 1024})
  if (result.status != 0) throw new Error(result.stderr || `comrak exited ${result.status}`)
  return result.stdout
}

const bySection = new Map
const divergences = []
for (const fixture of fixtures) {
  const lezerTree = projectLezer(lezer.parse(fixture.markdown).topNode)
  const comrakTree = projectComrak(renderComrak(fixture.markdown, fixture.profile))
  const section = bySection.get(fixture.section) || {total: 0, exact: 0, divergent: []}
  section.total++
  if (signature(lezerTree) == signature(comrakTree)) {
    section.exact++
  } else {
    section.divergent.push(fixture.example)
    divergences.push({
      section: fixture.section,
      example: fixture.example,
      lezer: lezerTree,
      comrak: comrakTree,
      markdown: fixture.markdown,
    })
  }
  bySection.set(fixture.section, section)
}

function typedAndErased(name, finalSource) {
  const boundaries = [0]
  for (let index = 0; index < finalSource.length;) {
    const width = finalSource.codePointAt(index) > 0xffff ? 2 : 1
    index += width
    boundaries.push(index)
  }
  const revisions = [""]
  for (const boundary of boundaries.slice(1)) revisions.push(finalSource.slice(0, boundary))
  for (const boundary of boundaries.slice(0, -1).reverse()) revisions.push(finalSource.slice(0, boundary))
  return {name, revisions}
}

const histories = [
  typedAndErased("quote-list-tab-every-revision", "> - item\n>\tcontinued\nlazy\n\nend\n"),
  typedAndErased("setext-every-revision", "heading\n=======\n\nafter\n"),
  typedAndErased("table-every-revision", "| a | `b\\|c` |\n| :- | -: |\n| d | e |\n\nafter\n"),
  typedAndErased("html-classes-every-revision", "<script>x</script>\n\n<!-- comment -->\n\n<?processing?>\n\n<!DECLARATION>\n\n<![CDATA[data]]>\n\n<div>\nraw *body*\n\n<i>raw</i>\n\n"),
  {name: "lazy-quote-interruption-toggle", revisions: [
    "> paragraph\ncontinuation\n\nafter\n",
    "> paragraph\n#continuation\n\nafter\n",
    "> paragraph\n# continuation\n\nafter\n",
    "> paragraph\ncontinuation\n\nafter\n",
  ]},
  {name: "list-tightness-toggle", revisions: [
    "- a\n- b\n\nafter\n",
    "- a\n\n- b\n\nafter\n",
    "- a\n- b\n\nafter\n",
  ]},
  {name: "setext-validity-toggle", revisions: [
    "title\n=====\n\nafter\n",
    "title\n==x==\n\nafter\n",
    "title\n=====\n\nafter\n",
    "changed\n=====\n\nafter\n",
  ]},
  {name: "table-validity-toggle", revisions: [
    "| a | b |\n| -- | -- |\n| c | d |\n\nafter\n",
    "| a | b |\n| x- | -- |\n| c | d |\n\nafter\n",
    "| a | b |\n| -- | -- |\n| c | d |\n\nafter\n",
    "| a | b |\n| -- | -- |\n> quote\n\nafter\n",
  ]},
  {name: "tab-indentation-toggle", revisions: [
    "- foo\n\n\tbar\n\nafter\n",
    "- foo\n\n   bar\n\nafter\n",
    "- foo\n\n    bar\n\nafter\n",
    "- foo\n\n\tbar\n\nafter\n",
  ]},
  {name: "html-close-toggle", revisions: [
    "<!--\n*inert*\n--x\nafter\n",
    "<!--\n*inert*\n-->\nafter\n",
    "<!--\n*inert*\n--x\nafter\n",
  ]},
]

function changedRange(before, after) {
  let from = 0
  while (from < before.length && from < after.length && before.charCodeAt(from) == after.charCodeAt(from)) from++
  let toA = before.length, toB = after.length
  while (toA > from && toB > from && before.charCodeAt(toA - 1) == after.charCodeAt(toB - 1)) {
    toA--
    toB--
  }
  return {fromA: from, toA, fromB: from, toB}
}

const historyRows = []
let historyRevisions = 0, historyStructuralDivergences = 0, historyIncrementalMismatches = 0
for (const history of histories) {
  let source = history.revisions[0]
  let tree = lezer.parse(source)
  let fragments = TreeFragment.addTree(tree)
  let structuralDivergences = 0, incrementalMismatches = 0
  for (let index = 0; index < history.revisions.length; index++) {
    const nextSource = history.revisions[index]
    if (index) {
      const changed = changedRange(source, nextSource)
      const nextFragments = TreeFragment.applyChanges(fragments, [changed], 2)
      tree = lezer.parse(nextSource, nextFragments)
      fragments = TreeFragment.addTree(tree, nextFragments)
      source = nextSource
    }
    const clean = lezer.parse(nextSource)
    if (exactTreeSignature(tree) != exactTreeSignature(clean)) incrementalMismatches++
    const lezerTree = projectLezer(clean.topNode)
    const comrakTree = projectComrak(renderComrak(nextSource, "gfm"))
    if (signature(lezerTree) != signature(comrakTree)) structuralDivergences++
  }
  historyRevisions += history.revisions.length
  historyStructuralDivergences += structuralDivergences
  historyIncrementalMismatches += incrementalMismatches
  historyRows.push({name: history.name, revisions: history.revisions.length, structuralDivergences, incrementalMismatches})
}

const report = {
  fixtureCount: fixtures.length,
  exact: fixtures.length - divergences.length,
  divergent: divergences.length,
  bySection: Object.fromEntries(bySection),
  divergenceIds: divergences.map(({section, example}) => ({section, example})),
  histories: {
    revisions: historyRevisions,
    structuralDivergences: historyStructuralDivergences,
    incrementalMismatches: historyIncrementalMismatches,
    rows: historyRows,
  },
}
if (detailFlag == "--details") report.divergences = divergences
console.log(JSON.stringify(report, null, 2))
