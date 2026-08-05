//! Test-only normalized HTML projection for the value block tree.
//!
//! Block structure is rendered directly from Flark-owned value state. Inline
//! bearing leaves are delegated only to Comrak's bounded inline-fragment
//! service; this module never invokes Comrak's block parser.

use std::collections::HashMap;
use std::fmt;

use comrak::block_spine_facade::{FacadeError, MAX_CLASSIFICATION_BYTES, normalize_code_info};
use comrak::html::{escape, escape_href};
use comrak::inline_fragment::{
    INLINE_FACT_FLAG_REFERENCE_SYMBOL, INLINE_FACT_FLAG_SOURCE_BACKED,
    INLINE_FACT_FLAG_TASK_CHECKED, InlineFact, InlineFactKind, InlineFragment, InlineFragmentError,
    InlineFragmentRequest, InlineInputKind, InlineProfile, InlineProjectionFactKind,
    InlineReferenceSnapshot, InlineReferenceTarget, parse_inline_fragment,
};

use crate::source::{LogicalChunk, LogicalProjection, ProjectionReadError};
use crate::tree::{
    Alignment, BlockDocument, BlockKind, ListType, NodeId, ReferenceOccurrence, SyntaxProfile,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderError {
    Inline(InlineFragmentError),
    Projection(ProjectionReadError),
    Facade(FacadeError),
    MalformedFact(&'static str),
    InvalidUtf8,
    InvalidBlockTree(&'static str),
}

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inline(error) => write!(formatter, "inline fragment failed: {error:?}"),
            Self::Projection(error) => write!(formatter, "source projection failed: {error:?}"),
            Self::Facade(error) => write!(formatter, "bounded block transform failed: {error:?}"),
            Self::MalformedFact(message) => write!(formatter, "malformed inline fact: {message}"),
            Self::InvalidUtf8 => formatter.write_str("inline fact payload was not UTF-8"),
            Self::InvalidBlockTree(message) => write!(formatter, "invalid block tree: {message}"),
        }
    }
}

impl std::error::Error for RenderError {}

impl From<InlineFragmentError> for RenderError {
    fn from(error: InlineFragmentError) -> Self {
        Self::Inline(error)
    }
}

impl From<ProjectionReadError> for RenderError {
    fn from(error: ProjectionReadError) -> Self {
        Self::Projection(error)
    }
}

impl From<FacadeError> for RenderError {
    fn from(error: FacadeError) -> Self {
        Self::Facade(error)
    }
}

/// Receipt for the only raw-block transform that must own an intermediate.
/// The parser's classification cap makes this allocation atomically bounded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodeInfoTransformReceipt {
    pub raw_bytes_copied: usize,
    pub normalized_bytes_owned: usize,
    pub input_cap: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundedCodeInfo {
    pub value: String,
    pub receipt: CodeInfoTransformReceipt,
}

/// Apply Comrak's exact entity/trim/backslash normalization to a fenced info
/// projection. The cap is checked before source bytes are materialized.
pub fn bounded_code_info(
    document: &BlockDocument,
    node: NodeId,
    projection: LogicalProjection,
) -> Result<BoundedCodeInfo, RenderError> {
    let input_bytes = usize::try_from(projection.len())
        .map_err(|_| RenderError::InvalidBlockTree("code info length overflow"))?;
    if input_bytes > MAX_CLASSIFICATION_BYTES {
        return Err(FacadeError::OverCap {
            bytes: input_bytes,
            cap: MAX_CLASSIFICATION_BYTES,
        }
        .into());
    }
    let raw = document.materialize_projection(node, projection)?;
    let value = normalize_code_info(&raw)?;
    Ok(BoundedCodeInfo {
        receipt: CodeInfoTransformReceipt {
            raw_bytes_copied: raw.len(),
            normalized_bytes_owned: value.len(),
            input_cap: MAX_CLASSIFICATION_BYTES,
        },
        value,
    })
}

/// Render the clean value tree with the exact default CommonMark/GFM HTML
/// serialization used by Gate A (`unsafe` raw HTML enabled).
///
/// # Errors
///
/// Returns [`RenderError`] when the value tree is malformed, an inline leaf
/// exceeds the bounded service contract, or a compact inline fact is invalid.
pub fn normalized_html(document: &BlockDocument) -> Result<String, RenderError> {
    let references = ReferenceSnapshot::new(&document.references);
    let mut renderer = Renderer {
        document,
        references,
        output: String::new(),
    };
    renderer.render_children(document.tree.root)?;
    Ok(renderer.output)
}

#[derive(Debug)]
struct ReferenceSnapshot<'a> {
    by_label: HashMap<&'a str, u64>,
    by_symbol: HashMap<u64, &'a ReferenceOccurrence>,
}

impl<'a> ReferenceSnapshot<'a> {
    fn new(occurrences: &'a [ReferenceOccurrence]) -> Self {
        let mut by_label = HashMap::new();
        let mut by_symbol = HashMap::new();
        for occurrence in occurrences {
            if by_label.contains_key(occurrence.normalized_label.as_str()) {
                continue;
            }
            // Defined symbols occupy the compact non-zero prefix and are
            // collision-free within this immutable snapshot.
            let symbol = u64::try_from(by_symbol.len() + 1).expect("reference symbol below u64");
            by_label.insert(occurrence.normalized_label.as_str(), symbol);
            by_symbol.insert(symbol, occurrence);
        }
        Self {
            by_label,
            by_symbol,
        }
    }

    fn value(&self, symbol: u64) -> Option<&'a ReferenceOccurrence> {
        self.by_symbol.get(&symbol).copied()
    }
}

impl InlineReferenceSnapshot for ReferenceSnapshot<'_> {
    fn identity(&self) -> u64 {
        1
    }

    fn generation(&self) -> u64 {
        1
    }

    fn resolve(&self, normalized: &str, _original: &str) -> InlineReferenceTarget {
        if let Some(symbol_id) = self.by_label.get(normalized).copied() {
            InlineReferenceTarget {
                symbol_id,
                presence_generation: 1,
                defined: true,
            }
        } else {
            InlineReferenceTarget {
                symbol_id: missing_symbol_id(normalized),
                presence_generation: 0,
                defined: false,
            }
        }
    }
}

fn missing_symbol_id(label: &str) -> u64 {
    label
        .as_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
        | (1_u64 << 63)
}

struct Renderer<'a> {
    document: &'a BlockDocument,
    references: ReferenceSnapshot<'a>,
    output: String,
}

impl Renderer<'_> {
    fn render_children(&mut self, parent: NodeId) -> Result<(), RenderError> {
        for child in self.document.tree.node(parent).children.clone() {
            self.render_block(child)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn render_block(&mut self, node: NodeId) -> Result<(), RenderError> {
        let block = self.document.tree.node(node);
        match &block.kind {
            BlockKind::Document => self.render_children(node),
            BlockKind::BlockQuote => {
                self.cr();
                self.write("<blockquote>");
                self.lf();
                self.render_children(node)?;
                self.cr();
                self.write("</blockquote>");
                self.lf();
                Ok(())
            }
            BlockKind::List(list) => {
                self.cr();
                match list.list_type {
                    ListType::Bullet => self.write("<ul>"),
                    ListType::Ordered if list.start == 1 => self.write("<ol>"),
                    ListType::Ordered => {
                        self.write("<ol start=\"");
                        self.write(&list.start.to_string());
                        self.write("\">");
                    }
                }
                self.lf();
                self.render_children(node)?;
                self.write(match list.list_type {
                    ListType::Bullet => "</ul>",
                    ListType::Ordered => "</ol>",
                });
                self.lf();
                Ok(())
            }
            BlockKind::Item(_) => {
                self.cr();
                self.write("<li>");
                self.render_children(node)?;
                self.write("</li>");
                self.lf();
                Ok(())
            }
            BlockKind::CodeBlock { info, literal, .. } => {
                let info = bounded_code_info(self.document, node, *info)?;
                self.cr();
                self.write("<pre><code");
                if !info.value.is_empty() {
                    let language_end = info
                        .value
                        .as_bytes()
                        .iter()
                        .position(u8::is_ascii_whitespace)
                        .unwrap_or(info.value.len());
                    self.write(" class=\"language-");
                    self.write_escaped(&info.value[..language_end]);
                    self.write("\"");
                }
                self.write(">");
                self.write_escaped_projection(node, *literal)?;
                self.write("</code></pre>");
                self.lf();
                Ok(())
            }
            BlockKind::HtmlBlock { literal, .. } => {
                self.cr();
                if self.document.profile == SyntaxProfile::Gfm {
                    self.write_tagfiltered_projection(node, *literal)?;
                } else {
                    self.write_projection(node, *literal)?;
                }
                self.cr();
                Ok(())
            }
            BlockKind::Paragraph => self.render_paragraph(node),
            BlockKind::Heading { level, setext, .. } => {
                self.cr();
                self.write("<h");
                self.write(&level.to_string());
                self.write(">");
                self.render_inline(
                    node,
                    InlineInputKind::Heading {
                        level: *level,
                        setext: *setext,
                    },
                )?;
                self.write("</h");
                self.write(&level.to_string());
                self.write(">");
                self.lf();
                Ok(())
            }
            BlockKind::ThematicBreak => {
                self.cr();
                self.write("<hr />");
                self.lf();
                Ok(())
            }
            BlockKind::Table(table) => self.render_table(node, &table.alignments),
            BlockKind::TableRow { .. } | BlockKind::TableCell => Err(
                RenderError::InvalidBlockTree("table row or cell escaped its table"),
            ),
        }
    }

    fn paragraph_is_tight(&self, paragraph: NodeId) -> bool {
        let Some(item) = self.document.tree.parent(paragraph) else {
            return false;
        };
        if !matches!(self.document.tree.node(item).kind, BlockKind::Item(_)) {
            return false;
        }
        let Some(list) = self.document.tree.parent(item) else {
            return false;
        };
        matches!(self.document.tree.node(list).kind, BlockKind::List(data) if data.tight)
    }

    fn paragraph_is_first_list_item_child(&self, paragraph: NodeId) -> bool {
        let Some(item) = self.document.tree.parent(paragraph) else {
            return false;
        };
        if !matches!(self.document.tree.node(item).kind, BlockKind::Item(_))
            || self.document.tree.first_child(item) != Some(paragraph)
        {
            return false;
        }
        let Some(list) = self.document.tree.parent(item) else {
            return false;
        };
        matches!(self.document.tree.node(list).kind, BlockKind::List(_))
    }

    fn render_paragraph(&mut self, node: NodeId) -> Result<(), RenderError> {
        let tight = self.paragraph_is_tight(node);
        let inline_kind = if self.paragraph_is_first_list_item_child(node) {
            InlineInputKind::ListItemParagraph
        } else {
            InlineInputKind::Paragraph
        };
        let fragment = self.parse_inline(node, inline_kind)?;
        let forest = FactForest::new(&fragment)?;
        let task_root = forest
            .roots
            .iter()
            .copied()
            .find(|root| forest.nodes[*root].fact.kind == InlineFactKind::TaskListMarker as u8);
        let logical = &self.document.tree.node(node).content.logical;

        // Comrak models the checkbox as the task-item wrapper rather than as
        // paragraph content. In a tight list those positions coincide. In a
        // loose list the checkbox must precede the first `<p>`, while the
        // paragraph still owns the remaining inline content.
        if !tight {
            if let Some(task) = task_root {
                self.render_fact(&fragment, logical, &forest.nodes, task, InlineMode::Html)?;
            }
            self.cr();
            self.write("<p>");
        }
        for root in forest.roots {
            if tight || Some(root) != task_root {
                self.render_fact(&fragment, logical, &forest.nodes, root, InlineMode::Html)?;
            }
        }
        if !tight {
            self.write("</p>");
            self.lf();
        }
        Ok(())
    }

    fn render_table(&mut self, table: NodeId, alignments: &[Alignment]) -> Result<(), RenderError> {
        self.cr();
        self.write("<table>");
        self.lf();
        let rows = self.document.tree.node(table).children.clone();
        let mut body_open = false;
        for row in &rows {
            let BlockKind::TableRow { header } = self.document.tree.node(*row).kind else {
                return Err(RenderError::InvalidBlockTree("non-row child of table"));
            };
            self.cr();
            if header {
                self.write("<thead>");
                self.lf();
            } else if !body_open {
                self.write("<tbody>");
                self.lf();
                body_open = true;
            }
            self.write("<tr>");
            let cells = self.document.tree.node(*row).children.clone();
            for (column, cell) in cells.into_iter().enumerate() {
                if !matches!(self.document.tree.node(cell).kind, BlockKind::TableCell) {
                    return Err(RenderError::InvalidBlockTree("non-cell child of table row"));
                }
                self.cr();
                self.write(if header { "<th" } else { "<td" });
                match alignments.get(column).copied().unwrap_or(Alignment::None) {
                    Alignment::None => {}
                    Alignment::Left => self.write(" align=\"left\""),
                    Alignment::Center => self.write(" align=\"center\""),
                    Alignment::Right => self.write(" align=\"right\""),
                }
                self.write(">");
                self.render_inline(cell, InlineInputKind::TableCell)?;
                self.write(if header { "</th>" } else { "</td>" });
            }
            self.cr();
            self.write("</tr>");
            if header {
                self.cr();
                self.write("</thead>");
            }
        }
        if body_open {
            self.cr();
            self.write("</tbody>");
            self.lf();
        }
        self.cr();
        self.write("</table>");
        self.lf();
        Ok(())
    }

    fn render_inline(&mut self, node: NodeId, kind: InlineInputKind) -> Result<(), RenderError> {
        let fragment = self.parse_inline(node, kind)?;
        let forest = FactForest::new(&fragment)?;
        let logical = &self.document.tree.node(node).content.logical;
        for root in forest.roots {
            self.render_fact(&fragment, logical, &forest.nodes, root, InlineMode::Html)?;
        }
        Ok(())
    }

    fn parse_inline(
        &self,
        node: NodeId,
        kind: InlineInputKind,
    ) -> Result<InlineFragment, RenderError> {
        let logical = &self.document.tree.node(node).content.logical;
        parse_inline_fragment(InlineFragmentRequest {
            logical,
            leaf_id: u64::from(node.0) + 1,
            kind,
            profile: match self.document.profile {
                SyntaxProfile::CommonMark => InlineProfile::CommonMark,
                SyntaxProfile::Gfm => InlineProfile::Gfm,
            },
            reference_snapshot: &self.references,
            revision: 1,
            expected_revision: 1,
        })
        .map_err(Into::into)
    }

    #[allow(clippy::too_many_lines)]
    fn render_fact(
        &mut self,
        fragment: &InlineFragment,
        logical: &str,
        nodes: &[FactNode],
        index: usize,
        mode: InlineMode,
    ) -> Result<(), RenderError> {
        let node = &nodes[index];
        let fact = node.fact;
        let kind = fact_kind(fact.kind)?;
        if mode == InlineMode::Plain {
            match kind {
                InlineFactKind::Text => {
                    let text = materialize_text(fragment, logical, fact)?;
                    self.write_escaped(&text);
                }
                InlineFactKind::Code => {
                    let payload = payload(fragment, fact)?;
                    let literal = payload
                        .get(4..)
                        .ok_or(RenderError::MalformedFact("short code payload"))?;
                    self.write_escaped(
                        str::from_utf8(literal).map_err(|_| RenderError::InvalidUtf8)?,
                    );
                }
                InlineFactKind::HtmlInline => {
                    self.write_escaped(payload_str(fragment, fact)?);
                }
                InlineFactKind::SoftBreak | InlineFactKind::LineBreak => self.write(" "),
                InlineFactKind::TaskListMarker => {}
                _ => {
                    for child in &node.children {
                        self.render_fact(fragment, logical, nodes, *child, InlineMode::Plain)?;
                    }
                }
            }
            return Ok(());
        }

        match kind {
            InlineFactKind::Text => {
                let text = materialize_text(fragment, logical, fact)?;
                self.write_escaped(&text);
            }
            InlineFactKind::SoftBreak => self.write("\n"),
            InlineFactKind::LineBreak => self.write("<br />\n"),
            InlineFactKind::Code => {
                let encoded = payload(fragment, fact)?;
                let literal = encoded
                    .get(4..)
                    .ok_or(RenderError::MalformedFact("short code payload"))?;
                self.write("<code>");
                self.write_escaped(str::from_utf8(literal).map_err(|_| RenderError::InvalidUtf8)?);
                self.write("</code>");
            }
            InlineFactKind::HtmlInline => {
                let literal = payload_str(fragment, fact)?;
                if self.document.profile == SyntaxProfile::Gfm && tagfilter(literal) {
                    self.write("&lt;");
                    self.write(&literal[1..]);
                } else {
                    self.write(literal);
                }
            }
            InlineFactKind::Emphasis => {
                self.render_container(fragment, logical, nodes, node, "<em>", "</em>")?;
            }
            InlineFactKind::Strong => {
                self.render_container(fragment, logical, nodes, node, "<strong>", "</strong>")?;
            }
            InlineFactKind::Strikethrough => {
                self.render_container(fragment, logical, nodes, node, "<del>", "</del>")?;
            }
            InlineFactKind::Link => {
                let (url, title) = self.link_value(fragment, fact)?;
                self.write("<a href=\"");
                self.write_href(&url);
                if title.is_empty() {
                    self.write("\">");
                } else {
                    self.write("\" title=\"");
                    self.write_escaped(&title);
                    self.write("\">");
                }
                for child in &node.children {
                    self.render_fact(fragment, logical, nodes, *child, InlineMode::Html)?;
                }
                self.write("</a>");
            }
            InlineFactKind::Image => {
                let (url, title) = self.link_value(fragment, fact)?;
                self.write("<img src=\"");
                self.write_href(&url);
                self.write("\" alt=\"");
                for child in &node.children {
                    self.render_fact(fragment, logical, nodes, *child, InlineMode::Plain)?;
                }
                if title.is_empty() {
                    self.write("\" />");
                } else {
                    self.write("\" title=\"");
                    self.write_escaped(&title);
                    self.write("\" />");
                }
            }
            InlineFactKind::Escaped => {
                for child in &node.children {
                    self.render_fact(fragment, logical, nodes, *child, InlineMode::Html)?;
                }
            }
            InlineFactKind::TaskListMarker => {
                self.write("<input type=\"checkbox\"");
                if fact.flags & INLINE_FACT_FLAG_TASK_CHECKED != 0 {
                    self.write(" checked=\"\"");
                }
                self.write(" disabled=\"\" /> ");
            }
        }
        Ok(())
    }

    fn render_container(
        &mut self,
        fragment: &InlineFragment,
        logical: &str,
        nodes: &[FactNode],
        node: &FactNode,
        open: &str,
        close: &str,
    ) -> Result<(), RenderError> {
        self.write(open);
        for child in &node.children {
            self.render_fact(fragment, logical, nodes, *child, InlineMode::Html)?;
        }
        self.write(close);
        Ok(())
    }

    fn link_value(
        &self,
        fragment: &InlineFragment,
        fact: InlineFact,
    ) -> Result<(String, String), RenderError> {
        let encoded = payload(fragment, fact)?;
        if fact.flags & INLINE_FACT_FLAG_REFERENCE_SYMBOL != 0 {
            let bytes: [u8; 8] = encoded
                .try_into()
                .map_err(|_| RenderError::MalformedFact("reference symbol is not u64"))?;
            let symbol = u64::from_le_bytes(bytes);
            let target = self
                .references
                .value(symbol)
                .ok_or(RenderError::MalformedFact("unknown reference symbol"))?;
            return Ok((target.url.clone(), target.title.clone()));
        }
        let url_len_bytes: [u8; 4] = encoded
            .get(..4)
            .ok_or(RenderError::MalformedFact("short link payload"))?
            .try_into()
            .expect("four-byte slice");
        let url_len = u32::from_le_bytes(url_len_bytes) as usize;
        let url_end = 4_usize
            .checked_add(url_len)
            .ok_or(RenderError::MalformedFact("link URL length overflow"))?;
        let url = encoded
            .get(4..url_end)
            .ok_or(RenderError::MalformedFact("link URL exceeds payload"))?;
        let title = encoded
            .get(url_end..)
            .ok_or(RenderError::MalformedFact("link title exceeds payload"))?;
        Ok((
            str::from_utf8(url)
                .map_err(|_| RenderError::InvalidUtf8)?
                .to_owned(),
            str::from_utf8(title)
                .map_err(|_| RenderError::InvalidUtf8)?
                .to_owned(),
        ))
    }

    fn write(&mut self, value: &str) {
        self.output.push_str(value);
    }

    fn cr(&mut self) {
        if !self.output.is_empty() && !self.output.ends_with('\n') {
            self.output.push('\n');
        }
    }

    fn lf(&mut self) {
        self.output.push('\n');
    }

    fn write_escaped(&mut self, value: &str) {
        escape(&mut self.output, value).expect("writing to String cannot fail");
    }

    fn write_href(&mut self, value: &str) {
        escape_href(&mut self.output, value, false).expect("writing to String cannot fail");
    }

    fn write_projection(
        &mut self,
        node: NodeId,
        projection: LogicalProjection,
    ) -> Result<(), RenderError> {
        let mut cursor = self.document.projection_cursor(node, projection)?;
        while let Some(chunk) = cursor.next_chunk()? {
            match chunk {
                LogicalChunk::Borrowed(text) => self.output.push_str(text),
                LogicalChunk::Spaces(count) => {
                    self.output.extend(std::iter::repeat_n(' ', count));
                }
                LogicalChunk::Newline => self.output.push('\n'),
            }
        }
        Ok(())
    }

    fn write_escaped_projection(
        &mut self,
        node: NodeId,
        projection: LogicalProjection,
    ) -> Result<(), RenderError> {
        let mut cursor = self.document.projection_cursor(node, projection)?;
        while let Some(chunk) = cursor.next_chunk()? {
            match chunk {
                LogicalChunk::Borrowed(text) => {
                    escape(&mut self.output, text).expect("writing to String cannot fail");
                }
                LogicalChunk::Spaces(count) => {
                    self.output.extend(std::iter::repeat_n(' ', count));
                }
                LogicalChunk::Newline => self.output.push('\n'),
            }
        }
        Ok(())
    }

    fn write_tagfiltered_projection(
        &mut self,
        node: NodeId,
        projection: LogicalProjection,
    ) -> Result<(), RenderError> {
        let mut cursor = self.document.projection_cursor(node, projection)?;
        while let Some(chunk) = cursor.next_chunk()? {
            match chunk {
                // Identity runs end at physical-line boundaries. A tag name
                // cannot continue across a line ending, so tagfilter's finite
                // lookahead remains exact without joining aggregate content.
                LogicalChunk::Borrowed(text) => write_tagfiltered(&mut self.output, text),
                LogicalChunk::Spaces(count) => {
                    self.output.extend(std::iter::repeat_n(' ', count));
                }
                LogicalChunk::Newline => self.output.push('\n'),
            }
        }
        Ok(())
    }
}

fn write_tagfiltered(output: &mut String, literal: &str) {
    let mut cursor = 0;
    while let Some(relative) = literal.as_bytes()[cursor..]
        .iter()
        .position(|byte| *byte == b'<')
    {
        let marker = cursor + relative;
        output.push_str(&literal[cursor..marker]);
        if tagfilter(&literal[marker..]) {
            output.push_str("&lt;");
        } else {
            output.push('<');
        }
        cursor = marker + 1;
    }
    output.push_str(&literal[cursor..]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InlineMode {
    Html,
    Plain,
}

#[derive(Clone, Debug)]
struct FactNode {
    fact: InlineFact,
    children: Vec<usize>,
}

struct FactForest {
    nodes: Vec<FactNode>,
    roots: Vec<usize>,
}

impl FactForest {
    fn new(fragment: &InlineFragment) -> Result<Self, RenderError> {
        let mut nodes = Vec::<FactNode>::with_capacity(fragment.facts.len());
        let mut roots = Vec::new();
        let mut stack = Vec::<(u16, usize)>::new();
        for fact in &fragment.facts {
            if fact.depth == 0 {
                if fact.kind != InlineFactKind::TaskListMarker as u8 {
                    return Err(RenderError::MalformedFact("semantic fact has zero depth"));
                }
                let index = nodes.len();
                nodes.push(FactNode {
                    fact: *fact,
                    children: Vec::new(),
                });
                roots.push(index);
                continue;
            }
            while stack.last().is_some_and(|(depth, _)| *depth >= fact.depth) {
                stack.pop();
            }
            if let Some((parent_depth, _)) = stack.last()
                && parent_depth + 1 != fact.depth
            {
                return Err(RenderError::MalformedFact("inline fact skipped a depth"));
            }
            if stack.is_empty() && fact.depth != 1 {
                return Err(RenderError::MalformedFact("inline root depth is not one"));
            }
            let index = nodes.len();
            nodes.push(FactNode {
                fact: *fact,
                children: Vec::new(),
            });
            if let Some((_, parent)) = stack.last().copied() {
                nodes[parent].children.push(index);
            } else {
                roots.push(index);
            }
            stack.push((fact.depth, index));
        }
        Ok(Self { nodes, roots })
    }
}

fn fact_kind(kind: u8) -> Result<InlineFactKind, RenderError> {
    match kind {
        value if value == InlineFactKind::Text as u8 => Ok(InlineFactKind::Text),
        value if value == InlineFactKind::SoftBreak as u8 => Ok(InlineFactKind::SoftBreak),
        value if value == InlineFactKind::LineBreak as u8 => Ok(InlineFactKind::LineBreak),
        value if value == InlineFactKind::Code as u8 => Ok(InlineFactKind::Code),
        value if value == InlineFactKind::HtmlInline as u8 => Ok(InlineFactKind::HtmlInline),
        value if value == InlineFactKind::Emphasis as u8 => Ok(InlineFactKind::Emphasis),
        value if value == InlineFactKind::Strong as u8 => Ok(InlineFactKind::Strong),
        value if value == InlineFactKind::Strikethrough as u8 => Ok(InlineFactKind::Strikethrough),
        value if value == InlineFactKind::Link as u8 => Ok(InlineFactKind::Link),
        value if value == InlineFactKind::Image as u8 => Ok(InlineFactKind::Image),
        value if value == InlineFactKind::Escaped as u8 => Ok(InlineFactKind::Escaped),
        value if value == InlineFactKind::TaskListMarker as u8 => {
            Ok(InlineFactKind::TaskListMarker)
        }
        _ => Err(RenderError::MalformedFact("unknown semantic fact kind")),
    }
}

fn payload(fragment: &InlineFragment, fact: InlineFact) -> Result<&[u8], RenderError> {
    let start = fact.payload_start as usize;
    let end = start
        .checked_add(fact.payload_len as usize)
        .ok_or(RenderError::MalformedFact("payload range overflow"))?;
    fragment
        .payload
        .get(start..end)
        .ok_or(RenderError::MalformedFact("payload range exceeds buffer"))
}

fn payload_str(fragment: &InlineFragment, fact: InlineFact) -> Result<&str, RenderError> {
    str::from_utf8(payload(fragment, fact)?).map_err(|_| RenderError::InvalidUtf8)
}

fn materialize_text(
    fragment: &InlineFragment,
    logical: &str,
    fact: InlineFact,
) -> Result<String, RenderError> {
    if fact.flags & INLINE_FACT_FLAG_SOURCE_BACKED == 0 {
        return payload_str(fragment, fact).map(ToOwned::to_owned);
    }
    let start = fact.logical_start as usize;
    let end = start
        .checked_add(fact.logical_len as usize)
        .ok_or(RenderError::MalformedFact("logical range overflow"))?;
    if logical.get(start..end).is_none() {
        return Err(RenderError::MalformedFact("logical range exceeds leaf"));
    }
    let mut projections = fragment
        .projection_facts
        .iter()
        .filter(|projection| {
            let projection_start = projection.logical_start as usize;
            let projection_end = projection_start + projection.logical_len as usize;
            projection_start >= start && projection_end <= end
        })
        .copied()
        .collect::<Vec<_>>();
    projections.sort_by_key(|projection| (projection.logical_start, projection.kind));

    let mut bytes = Vec::with_capacity(end - start);
    let mut cursor = start;
    for projection in projections {
        let projection_start = projection.logical_start as usize;
        let projection_end = projection_start
            .checked_add(projection.logical_len as usize)
            .ok_or(RenderError::MalformedFact("projection range overflow"))?;
        if projection_start < cursor || projection_end > end {
            return Err(RenderError::MalformedFact("overlapping projection facts"));
        }
        bytes.extend_from_slice(&logical.as_bytes()[cursor..projection_start]);
        if projection.kind == InlineProjectionFactKind::Replacement as u8 {
            bytes.extend_from_slice(payload(fragment, projection)?);
        } else if projection.kind != InlineProjectionFactKind::HiddenMarker as u8 {
            return Err(RenderError::MalformedFact("unknown projection fact kind"));
        }
        cursor = projection_end;
    }
    bytes.extend_from_slice(&logical.as_bytes()[cursor..end]);
    String::from_utf8(bytes).map_err(|_| RenderError::InvalidUtf8)
}

fn tagfilter(literal: &str) -> bool {
    const BLACKLIST: [&str; 9] = [
        "title",
        "textarea",
        "style",
        "xmp",
        "iframe",
        "noembed",
        "noframes",
        "script",
        "plaintext",
    ];
    let bytes = literal.as_bytes();
    if bytes.len() < 3 || bytes[0] != b'<' {
        return false;
    }
    let mut index = 1;
    if bytes[index] == b'/' {
        index += 1;
    }
    let lower = literal[index..].to_lowercase();
    for tag in BLACKLIST {
        if !lower.starts_with(tag) {
            continue;
        }
        let boundary = index + tag.len();
        let Some(byte) = bytes.get(boundary).copied() else {
            return false;
        };
        return byte.is_ascii_whitespace()
            || byte == b'>'
            || (byte == b'/' && bytes.get(boundary + 1) == Some(&b'>'));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceDocument;
    use crate::tree::{BlockTree, Position};

    fn one_leaf_document(
        profile: SyntaxProfile,
        kind: BlockKind,
        logical: &str,
        references: Vec<ReferenceOccurrence>,
    ) -> BlockDocument {
        let mut tree = BlockTree::new();
        let leaf = tree.append(tree.root, kind, Position::new(1, 1));
        tree.node_mut(leaf).content.logical = logical.to_owned();
        tree.close(leaf);
        tree.close(tree.root);
        BlockDocument {
            profile,
            source: SourceDocument::new(logical),
            tree,
            references,
        }
    }

    #[test]
    fn inline_facts_render_without_the_stock_block_parser() {
        let document = one_leaf_document(
            SyntaxProfile::Gfm,
            BlockKind::Paragraph,
            "plain &copy; *em* **strong** ~~gone~~ `code` [link](u \"t\") ![alt *x*](i)",
            Vec::new(),
        );
        assert_eq!(
            normalized_html(&document).unwrap(),
            "<p>plain © <em>em</em> <strong>strong</strong> <del>gone</del> <code>code</code> <a href=\"u\" title=\"t\">link</a> <img src=\"i\" alt=\"alt x\" /></p>\n"
        );
    }

    #[test]
    fn reference_snapshot_is_first_definition_wins() {
        let references = vec![
            ReferenceOccurrence {
                normalized_label: "x".to_owned(),
                url: "/first".to_owned(),
                title: "one".to_owned(),
                origins: Vec::new(),
            },
            ReferenceOccurrence {
                normalized_label: "x".to_owned(),
                url: "/second".to_owned(),
                title: "two".to_owned(),
                origins: Vec::new(),
            },
        ];
        let document = one_leaf_document(
            SyntaxProfile::CommonMark,
            BlockKind::Paragraph,
            "[text][x]",
            references,
        );
        assert_eq!(
            normalized_html(&document).unwrap(),
            "<p><a href=\"/first\" title=\"one\">text</a></p>\n"
        );
    }
}
