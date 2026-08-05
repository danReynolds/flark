#!/usr/bin/env node

// Reproducible function/line audit for the mechanically corresponding
// cmark-gfm block and GFM-table donor surface. This is source analysis only.

import {readFileSync} from "node:fs"
import {spawnSync} from "node:child_process"

const root = process.argv[2]
if (!root) throw new Error("usage: cmark_surface_probe.mjs CMARK_GFM_ROOT")

const blockNames = new Set([
  "S_advance_offset", "S_ends_with_blank_line", "S_find_first_nonspace",
  "S_is_line_end_char", "S_is_space_or_tab", "S_last_child_is_open",
  "S_last_line_blank", "S_last_line_checked", "S_process_line",
  "S_scan_thematic_break", "S_set_last_line_blank", "S_set_last_line_checked",
  "S_type", "accepts_lines", "add_child", "add_line",
  "add_text_to_container", "check_open_blocks", "chop_trailing_hashtags",
  "contains_inlines", "finalize", "finalize_document", "is_blank",
  "lists_match", "make_block", "make_document", "open_new_blocks",
  "parse_block_quote_prefix", "parse_code_block_prefix", "parse_extension_block",
  "parse_html_block_prefix", "parse_list_marker", "parse_node_item_prefix",
  "remove_trailing_blank_lines", "resolve_reference_link_definitions",
])

const tableNames = new Set([
  "append_row_cell", "free_node_table", "free_node_table_row", "free_row_cells",
  "free_table_cell", "free_table_row", "get_cell_alignment",
  "get_n_autocompleted_cells", "get_n_table_columns", "get_table_alignments",
  "incr_table_row_count", "matches", "row_from_string", "set_cell_index",
  "set_n_table_columns", "set_table_alignments",
  "try_inserting_table_header_paragraph", "try_opening_table_block",
  "try_opening_table_header", "try_opening_table_row", "unescape_pipes",
])

function startsFor(path) {
  const result = spawnSync("ctags", ["-x", path], {encoding: "utf8"})
  if (result.status != 0) throw new Error(result.stderr)
  const starts = new Map
  for (const row of result.stdout.trim().split("\n")) {
    const match = /^(\S+)\s+(\d+)\s+/.exec(row)
    if (match) starts.set(match[1], Number(match[2]))
  }
  return starts
}

function functionRange(source, startLine) {
  let offset = 0
  for (let line = 1; line < startLine; line++) offset = source.indexOf("\n", offset) + 1
  let brace = source.indexOf("{", offset), depth = 0, state = "code"
  if (brace < 0) throw new Error(`no opening brace at line ${startLine}`)
  for (let i = brace; i < source.length; i++) {
    const ch = source[i], next = source[i + 1]
    if (state == "line") {
      if (ch == "\n") state = "code"
    } else if (state == "block") {
      if (ch == "*" && next == "/") { state = "code"; i++ }
    } else if (state == "string") {
      if (ch == "\\") i++
      else if (ch == "\"") state = "code"
    } else if (state == "char") {
      if (ch == "\\") i++
      else if (ch == "'") state = "code"
    } else if (ch == "/" && next == "/") {
      state = "line"; i++
    } else if (ch == "/" && next == "*") {
      state = "block"; i++
    } else if (ch == "\"") {
      state = "string"
    } else if (ch == "'") {
      state = "char"
    } else if (ch == "{") {
      depth++
    } else if (ch == "}" && --depth == 0) {
      return [offset, i + 1]
    }
  }
  throw new Error(`unterminated function at line ${startLine}`)
}

function countLines(source, from, to) {
  let lines = 1
  for (let i = from; i < to; i++) if (source.charCodeAt(i) == 10) lines++
  return lines
}

function audit(module, path, selected) {
  const source = readFileSync(path, "utf8"), starts = startsFor(path), rows = []
  for (const name of selected) {
    const start = starts.get(name)
    if (!start) throw new Error(`missing ${name} in ${path}`)
    const [from, to] = functionRange(source, start)
    const body = source.slice(from, to)
    rows.push({
      module,
      name,
      start,
      lines: countLines(source, from, to),
      treeSites: (body.match(/->(?:parent|first_child|last_child|next|prev)|cmark_node_(?:insert|set_type|free|can_contain)/g) || []).length,
      contentSites: (body.match(/->content|cmark_strbuf|cmark_chunk_buf_detach|cmark_arena/g) || []).length,
    })
  }
  return rows.sort((a, b) => a.start - b.start)
}

const rows = [
  ...audit("src/blocks.c", `${root}/src/blocks.c`, blockNames),
  ...audit("extensions/table.c", `${root}/extensions/table.c`, tableNames),
]
const sum = key => rows.reduce((total, row) => total + row[key], 0)
console.log(JSON.stringify({
  selectedFunctions: rows.length,
  selectedFunctionLines: sum("lines"),
  directTreeSites: sum("treeSites"),
  directOwnedContentSites: sum("contentSites"),
  modules: {
    "src/blocks.c": rows.filter(row => row.module == "src/blocks.c").reduce((result, row) => ({functions: result.functions + 1, lines: result.lines + row.lines}), {functions: 0, lines: 0}),
    "extensions/table.c": rows.filter(row => row.module == "extensions/table.c").reduce((result, row) => ({functions: result.functions + 1, lines: result.lines + row.lines}), {functions: 0, lines: 0}),
  },
  rows,
}, null, 2))
