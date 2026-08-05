//! Audits the exact Comrak v0.54 block-parser surface that a source-backed,
//! value-state Flark engine would have to preserve or refactor.
//!
//! This is intentionally a source audit, not a parser. It distinguishes the
//! small event-observation seam from the much larger representation seam and
//! makes the maintenance estimate reproducible against another Comrak tree.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::{ImplItem, Item, ItemFn, ItemImpl, Type};

const BLOCK_METHODS: &[&str] = &[
    "new",
    "parse",
    "process_line",
    "check_open_blocks",
    "check_open_blocks_inner",
    "find_first_nonspace",
    "parse_block_quote_prefix",
    "is_greentext",
    "parse_node_item_prefix",
    "parse_code_block_prefix",
    "parse_html_block_prefix",
    "fix_zero_end_columns",
    "open_new_blocks",
    "handle_blockquote",
    "detect_blockquote",
    "handle_atx_heading",
    "detect_atx_heading",
    "handle_code_fence",
    "detect_code_fence",
    "handle_html_block",
    "detect_html_block",
    "handle_setext_heading",
    "detect_setext_heading",
    "handle_thematic_break",
    "detect_thematic_break",
    "scan_thematic_break_inner",
    "handle_list",
    "detect_list",
    "handle_code_block",
    "detect_code_block",
    "handle_table",
    "detect_table",
    "advance_offset",
    "add_child",
    "add_text_to_container",
    "add_line",
    "finalize_document",
    "propagate_list_sourcepos",
    "finalize",
    "resolve_reference_link_definitions",
    "finalize_borrowed",
    "determine_list_tight",
    "parse_reference_inline",
];

const BLOCK_FREE_FUNCTIONS: &[&str] = &["parse_list_marker", "lists_match", "byte_matches"];

const TABLE_FUNCTIONS: &[&str] = &[
    "try_opening_block",
    "try_opening_header",
    "try_opening_row",
    "row",
    "try_inserting_table_header_paragraph",
    "unescape_pipes",
    "adjust_table_counters",
    "get_num_autocompleted_cells",
    "matches",
];

const TREE_CALLS: &[&str] = &[
    ".data(",
    ".data_mut(",
    ".parent(",
    ".last_child(",
    ".last_child_is_open(",
    ".first_child(",
    ".next_sibling(",
    ".previous_sibling(",
    ".append(",
    ".prepend(",
    ".insert_after(",
    ".insert_before(",
    ".detach(",
    ".same_node(",
    ".descendants(",
    ".children(",
    ".extend(",
    ".can_contain_type(",
    ".ends_with_blank_line(",
];

const OWNERSHIP_TERMS: &[&str] = &[
    "Ast::new",
    ".arena.alloc",
    ".content",
    "content:",
    "line_offsets",
    "last_line_blank",
    "table_visited",
    ".open",
];

#[derive(Clone, Debug)]
struct FunctionAudit {
    module: &'static str,
    name: String,
    start: usize,
    end: usize,
    tree_sites: usize,
    ownership_sites: usize,
    scanner_sites: usize,
    source_state_sites: usize,
}

impl FunctionAudit {
    fn lines(&self) -> usize {
        self.end - self.start + 1
    }

    fn representation_coupled(&self) -> bool {
        self.tree_sites + self.ownership_sites > 0
    }
}

fn main() {
    let root = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_comrak_root);
    let parser = root.join("src/parser/mod.rs");
    let table = root.join("src/parser/table.rs");
    let nodes = root.join("src/nodes.rs");

    let mut audits = Vec::new();
    audits.extend(audit_parser(&parser));
    audits.extend(audit_free_functions(
        "parser/mod.rs",
        &parser,
        BLOCK_FREE_FUNCTIONS,
    ));
    audits.extend(audit_free_functions(
        "parser/table.rs",
        &table,
        TABLE_FUNCTIONS,
    ));

    let selected: BTreeSet<_> = audits.iter().map(|a| a.name.as_str()).collect();
    for required in BLOCK_METHODS
        .iter()
        .chain(BLOCK_FREE_FUNCTIONS)
        .chain(TABLE_FUNCTIONS)
    {
        assert!(
            selected.contains(required),
            "missing selected function {required}"
        );
    }

    let total_lines: usize = audits.iter().map(FunctionAudit::lines).sum();
    let coupled_lines: usize = audits
        .iter()
        .filter(|audit| audit.representation_coupled())
        .map(FunctionAudit::lines)
        .sum();
    let coupled_functions = audits
        .iter()
        .filter(|audit| audit.representation_coupled())
        .count();
    let tree_sites: usize = audits.iter().map(|audit| audit.tree_sites).sum();
    let ownership_sites: usize = audits.iter().map(|audit| audit.ownership_sites).sum();
    let scanner_sites: usize = audits.iter().map(|audit| audit.scanner_sites).sum();
    let source_state_sites: usize = audits.iter().map(|audit| audit.source_state_sites).sum();

    let nodes_source = fs::read_to_string(&nodes).expect("read nodes.rs");
    let node_contract_sites = count_all(
        &nodes_source,
        &[
            "pub enum NodeValue",
            "pub struct Ast",
            "pub type AstNode",
            "pub type Node",
            "can_contain_type",
            "last_child_is_open",
            "ends_with_blank_line",
        ],
    );

    println!("comrak_root={}", root.display());
    println!("selected_functions={}", audits.len());
    println!("selected_upstream_function_lines={total_lines}");
    println!("representation_coupled_functions={coupled_functions}");
    println!("representation_coupled_function_lines={coupled_lines}");
    println!("direct_tree_operation_sites={tree_sites}");
    println!("direct_owned_content_state_sites={ownership_sites}");
    println!("direct_source_position_state_sites={source_state_sites}");
    println!("generated_scanner_call_sites={scanner_sites}");
    println!("nodes_contract_landmarks={node_contract_sites}");

    let mut modules: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for audit in &audits {
        let entry = modules.entry(audit.module).or_default();
        entry.0 += 1;
        entry.1 += audit.lines();
    }
    for (module, (functions, lines)) in modules {
        println!("module={module} functions={functions} lines={lines}");
    }

    println!("\nfunction,module,lines,coupled,tree,owned,source_state,scanners");
    audits.sort_by_key(|audit| (audit.module, audit.start));
    for audit in audits {
        println!(
            "{},{},{},{},{},{},{},{}",
            audit.name,
            audit.module,
            audit.lines(),
            audit.representation_coupled(),
            audit.tree_sites,
            audit.ownership_sites,
            audit.source_state_sites,
            audit.scanner_sites,
        );
    }
}

fn default_comrak_root() -> PathBuf {
    PathBuf::from(env!("HOME"))
        .join(".cargo/registry/src/index.crates.io-1949cf8c6b5b557f/comrak-0.54.0")
}

fn audit_parser(path: &Path) -> Vec<FunctionAudit> {
    let source = fs::read_to_string(path).expect("read parser/mod.rs");
    let syntax = syn::parse_file(&source).expect("parse parser/mod.rs");
    let selected: BTreeSet<_> = BLOCK_METHODS.iter().copied().collect();
    let mut result = Vec::new();
    for item in syntax.items {
        let Item::Impl(item_impl) = item else {
            continue;
        };
        if !is_parser_impl(&item_impl) {
            continue;
        }
        for item in item_impl.items {
            let ImplItem::Fn(method) = item else {
                continue;
            };
            let name = method.sig.ident.to_string();
            if selected.contains(name.as_str()) {
                result.push(audit_span("parser/mod.rs", &source, name, method.span()));
            }
        }
    }
    result
}

fn is_parser_impl(item: &ItemImpl) -> bool {
    let Type::Path(path) = item.self_ty.as_ref() else {
        return false;
    };
    path.path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "Parser")
}

fn audit_free_functions(module: &'static str, path: &Path, names: &[&str]) -> Vec<FunctionAudit> {
    let source = fs::read_to_string(path).unwrap_or_else(|_| panic!("read {}", path.display()));
    let syntax = syn::parse_file(&source).unwrap_or_else(|_| panic!("parse {}", path.display()));
    let selected: BTreeSet<_> = names.iter().copied().collect();
    syntax
        .items
        .into_iter()
        .filter_map(|item| match item {
            Item::Fn(item_fn) if selected.contains(item_fn.sig.ident.to_string().as_str()) => {
                Some(audit_item_fn(module, &source, item_fn))
            }
            _ => None,
        })
        .collect()
}

fn audit_item_fn(module: &'static str, source: &str, item: ItemFn) -> FunctionAudit {
    audit_span(module, source, item.sig.ident.to_string(), item.span())
}

fn audit_span(module: &'static str, source: &str, name: String, span: Span) -> FunctionAudit {
    let start = span.start().line;
    let end = span.end().line;
    let body = source
        .lines()
        .skip(start.saturating_sub(1))
        .take(end - start + 1)
        .collect::<Vec<_>>()
        .join("\n");
    FunctionAudit {
        module,
        name,
        start,
        end,
        tree_sites: count_all(&body, TREE_CALLS),
        ownership_sites: count_all(&body, OWNERSHIP_TERMS),
        scanner_sites: body.matches("scanners::").count(),
        source_state_sites: count_all(
            &body,
            &["sourcepos", "line_number", "offset", "column", "curline_"],
        ),
    }
}

fn count_all(haystack: &str, needles: &[&str]) -> usize {
    needles
        .iter()
        .map(|needle| haystack.matches(needle).count())
        .sum()
}
