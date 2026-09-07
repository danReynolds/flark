use std::{cell::OnceCell, rc::Rc};
use tree_sitter_highlight::HighlightConfiguration;

pub const COUNT: u32 = 14;

pub struct Grammar {
    pub language: tree_sitter::Language,
    pub edit: tree_sitter::Query,
    pub detection: tree_sitter::Query,
    pub statement_context: Option<(&'static str, &'static str)>,
    name: &'static str,
    highlights: String,
    locals: &'static str,
    highlight: OnceCell<Result<HighlightGrammar, String>>,
}

pub struct HighlightGrammar {
    pub config: HighlightConfiguration,
    pub scopes: Vec<String>,
}

impl Grammar {
    // First indentation must not compile the much larger highlighting query.
    // In the worker lane that initialization belongs entirely to the worker.
    pub fn highlight(&self) -> Result<&HighlightGrammar, String> {
        self.highlight
            .get_or_init(|| {
                // Embedded-language injections remain a separate capability.
                let mut config = HighlightConfiguration::new(
                    self.language.clone(),
                    self.name,
                    &self.highlights,
                    "",
                    self.locals,
                )
                .map_err(|e| e.to_string())?;
                let scopes: Vec<String> = config
                    .query
                    .capture_names()
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
                config.configure(&scopes);
                Ok(HighlightGrammar { config, scopes })
            })
            .as_ref()
            .map_err(Clone::clone)
    }
}

// Each grammar and its highlight/locals queries come from one pinned crate.
// Characters which can finish a reindent token for each editing profile.
pub fn reindent_triggers(id: u32) -> &'static str {
    match id {
        1 | 2 | 6 | 7 | 8 | 9 | 10 | 14 => "})]",
        3 => "})]:",
        4 => "}]",
        5 => "})]defn",
        11 => "})]iencf",
        12 | 13 => ">",
        _ => "",
    }
}

// Branch keywords can only finish when this character is typed. Other
// triggers are closers, which can outdent only at the start of a line.
pub fn branch_triggers(id: u32) -> &'static str {
    match id {
        3 => ":",
        5 => "defn",
        11 => "iencf",
        12 | 13 => ">",
        _ => "",
    }
}

thread_local! {
    static GRAMMARS: [OnceCell<Result<Rc<Grammar>, String>>; COUNT as usize] = const { [const { OnceCell::new() }; COUNT as usize] };
}

pub fn get(id: u32) -> Result<Rc<Grammar>, String> {
    GRAMMARS.with(|grammars| {
        let slot = id
            .checked_sub(1)
            .and_then(|i| grammars.get(i as usize))
            .ok_or("unsupported language")?;
        slot.get_or_init(|| {
            let (language, name, highlights, locals) = match id {
                1 => (
                    tree_sitter_dart::LANGUAGE,
                    "dart",
                    tree_sitter_dart::HIGHLIGHTS_QUERY,
                    tree_sitter_dart::LOCALS_QUERY,
                ),
                2 => (
                    tree_sitter_javascript::LANGUAGE,
                    "javascript",
                    tree_sitter_javascript::HIGHLIGHT_QUERY,
                    tree_sitter_javascript::LOCALS_QUERY,
                ),
                3 => (
                    tree_sitter_python::LANGUAGE,
                    "python",
                    tree_sitter_python::HIGHLIGHTS_QUERY,
                    "",
                ),
                4 => (
                    tree_sitter_yaml::LANGUAGE,
                    "yaml",
                    tree_sitter_yaml::HIGHLIGHTS_QUERY,
                    "",
                ),
                5 => (
                    tree_sitter_ruby::LANGUAGE,
                    "ruby",
                    tree_sitter_ruby::HIGHLIGHTS_QUERY,
                    "",
                ),
                6 => (
                    tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
                    "typescript",
                    tree_sitter_typescript::HIGHLIGHTS_QUERY,
                    tree_sitter_typescript::LOCALS_QUERY,
                ),
                7 => (
                    tree_sitter_rust::LANGUAGE,
                    "rust",
                    tree_sitter_rust::HIGHLIGHTS_QUERY,
                    "",
                ),
                8 => (
                    tree_sitter_go::LANGUAGE,
                    "go",
                    tree_sitter_go::HIGHLIGHTS_QUERY,
                    "",
                ),
                9 => (
                    tree_sitter_json::LANGUAGE,
                    "json",
                    tree_sitter_json::HIGHLIGHTS_QUERY,
                    "",
                ),
                10 => (
                    tree_sitter_css::LANGUAGE,
                    "css",
                    tree_sitter_css::HIGHLIGHTS_QUERY,
                    "",
                ),
                11 => (
                    tree_sitter_bash::LANGUAGE,
                    "bash",
                    tree_sitter_bash::HIGHLIGHT_QUERY,
                    "",
                ),
                12 => (
                    tree_sitter_html::LANGUAGE,
                    "html",
                    tree_sitter_html::HIGHLIGHTS_QUERY,
                    "",
                ),
                13 => (
                    tree_sitter_xml::LANGUAGE_XML,
                    "xml",
                    tree_sitter_xml::XML_HIGHLIGHT_QUERY,
                    "",
                ),
                14 => (
                    tree_sitter_sequel::LANGUAGE,
                    "sql",
                    tree_sitter_sequel::HIGHLIGHTS_QUERY,
                    "",
                ),
                _ => unreachable!(),
            };
            let highlights = if id == 6 {
                format!(
                    "{}\n{}",
                    highlights,
                    tree_sitter_javascript::HIGHLIGHT_QUERY
                )
            } else {
                highlights.to_string()
            };
            let language = language.into();
            let editing = match id {
                1 => include_str!("../queries/dart.scm"),
                2 => include_str!("../queries/javascript.scm"),
                3 => include_str!("../queries/python.scm"),
                4 => include_str!("../queries/yaml.scm"),
                5 => include_str!("../queries/ruby.scm"),
                6 => include_str!("../queries/typescript.scm"),
                7 => include_str!("../queries/rust.scm"),
                8 => include_str!("../queries/go.scm"),
                9 => include_str!("../queries/json.scm"),
                10 => include_str!("../queries/css.scm"),
                11 => include_str!("../queries/bash.scm"),
                12 => include_str!("../queries/html.scm"),
                13 => include_str!("../queries/xml.scm"),
                14 => include_str!("../queries/sql.scm"),

                _ => unreachable!(),
            };
            let detection_source = match id {
                1 => include_str!("../queries/detect/dart.scm"),
                2 => include_str!("../queries/detect/javascript.scm"),
                3 => include_str!("../queries/detect/python.scm"),
                4 => include_str!("../queries/detect/yaml.scm"),
                5 => include_str!("../queries/detect/ruby.scm"),
                6 => include_str!("../queries/detect/typescript.scm"),
                7 => include_str!("../queries/detect/rust.scm"),
                8 => include_str!("../queries/detect/go.scm"),
                9 => include_str!("../queries/detect/json.scm"),
                10 => include_str!("../queries/detect/css.scm"),
                11 => include_str!("../queries/detect/bash.scm"),
                12 => include_str!("../queries/detect/html.scm"),
                13 => include_str!("../queries/detect/xml.scm"),
                14 => include_str!("../queries/detect/sql.scm"),
                _ => unreachable!(),
            };
            let detection =
                tree_sitter::Query::new(&language, detection_source).map_err(|e| e.to_string())?;
            for name in detection.capture_names() {
                if !matches!(*name, "signal" | "signal.weak" | "signal.strong") {
                    return Err(format!("unsupported detection capture: {name}"));
                }
            }
            let edit = tree_sitter::Query::new(&language, editing).map_err(|e| e.to_string())?;
            for name in edit.capture_names() {
                if !matches!(
                    *name,
                    "opaque"
                        | "opaque.tail"
                        | "opaque.body"
                        | "code"
                        | "comment"
                        | "indent.end"
                        | "indent.scalar"
                        | "indent.prefix"
                        | "open.block"
                        | "close.block"
                        | "middle.block"
                        | "open.paren"
                        | "open.bracket"
                        | "open.brace"
                        | "close.paren"
                        | "close.bracket"
                        | "close.brace"
                ) && !name.starts_with("anchor.")
                    && !name.starts_with("branch.")
                    && !name.starts_with("middle.")
                    && !name.starts_with("open.")
                    && !name.starts_with("close.")
                    && !name.starts_with("_")
                {
                    return Err(format!("unsupported editing capture: {name}"));
                }
            }
            for query in [&edit, &detection] {
                for i in 0..query.pattern_count() {
                    if !query.general_predicates(i).is_empty()
                        || !query.property_settings(i).is_empty()
                        || !query.property_predicates(i).is_empty()
                    {
                        return Err("unsupported snippet query predicate/property".into());
                    }
                }
            }
            let statement_context = if id == 1 {
                Some(("void __flark_snippet__() {\n", "\n}"))
            } else if id == 10 {
                Some((".flark_snippet {\n", "\n}"))
            } else {
                None
            };
            Ok(Rc::new(Grammar {
                language,
                edit,
                detection,
                statement_context,
                name,
                highlights,
                locals,
                highlight: OnceCell::new(),
            }))
        })
        .clone()
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn all_catalog_queries_compile() {
        let errors: Vec<_> = (1..=super::COUNT)
            .filter_map(|id| super::get(id).err().map(|e| format!("{id}: {e}")))
            .collect();
        assert!(errors.is_empty(), "{}", errors.join("\n"));
    }
}
