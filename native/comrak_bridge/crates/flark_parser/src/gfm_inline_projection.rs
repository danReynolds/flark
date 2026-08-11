//! Exact bounded GFM inline projection for complex logical leaves.
//!
//! The ordinary inline job remains the resumable/source-coordinate fast path.
//! This projector owns the exact semantic fallback for already-established
//! logical leaves that fit the donor's 8 KiB atomic ceiling. It exposes typed
//! values only; Comrak arena nodes and its final renderer never cross the seam.

use comrak::block_spine_facade::{
    self, FacadeInlineNode, FacadeInlineOptions, FacadeInlineReference,
};

pub const M11_GFM_INLINE_PROJECTION_MAX_BYTES: usize = block_spine_facade::MAX_CLASSIFICATION_BYTES;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct M11GfmInlineOptions {
    pub strikethrough: bool,
    pub autolink: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct M11GfmInlineReference {
    pub normalized_label: String,
    pub destination: String,
    pub title: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum M11GfmInlineNode {
    Text(String),
    SoftBreak,
    LineBreak,
    Code(String),
    Html(String),
    Transparent(Vec<M11GfmInlineNode>),
    Emphasis(Vec<M11GfmInlineNode>),
    Strong(Vec<M11GfmInlineNode>),
    Strikethrough(Vec<M11GfmInlineNode>),
    Link {
        destination: String,
        title: String,
        children: Vec<M11GfmInlineNode>,
    },
    Image {
        destination: String,
        title: String,
        children: Vec<M11GfmInlineNode>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum M11GfmInlineProjectionError {
    OverCap { bytes: usize, cap: usize },
    Unsupported,
}

pub fn project_m11_gfm_inline(
    logical_source: &str,
    options: M11GfmInlineOptions,
    references: &[M11GfmInlineReference],
) -> Result<Vec<M11GfmInlineNode>, M11GfmInlineProjectionError> {
    let references = references
        .iter()
        .map(|reference| FacadeInlineReference {
            normalized_label: reference.normalized_label.clone(),
            destination: reference.destination.clone(),
            title: reference.title.clone(),
        })
        .collect::<Vec<_>>();
    block_spine_facade::inline_projection(
        logical_source,
        FacadeInlineOptions {
            strikethrough: options.strikethrough,
            autolink: options.autolink,
        },
        &references,
    )
    .map(|nodes| nodes.into_iter().map(convert_node).collect())
    .map_err(|error| match error {
        block_spine_facade::FacadeInlineError::OverCap { bytes, cap } => {
            M11GfmInlineProjectionError::OverCap { bytes, cap }
        }
        _ => M11GfmInlineProjectionError::Unsupported,
    })
}

fn convert_node(node: FacadeInlineNode) -> M11GfmInlineNode {
    match node {
        FacadeInlineNode::Text(value) => M11GfmInlineNode::Text(value),
        FacadeInlineNode::SoftBreak => M11GfmInlineNode::SoftBreak,
        FacadeInlineNode::LineBreak => M11GfmInlineNode::LineBreak,
        FacadeInlineNode::Code(value) => M11GfmInlineNode::Code(value),
        FacadeInlineNode::Html(value) => M11GfmInlineNode::Html(value),
        FacadeInlineNode::Transparent(children) => {
            M11GfmInlineNode::Transparent(children.into_iter().map(convert_node).collect())
        }
        FacadeInlineNode::Emphasis(children) => {
            M11GfmInlineNode::Emphasis(children.into_iter().map(convert_node).collect())
        }
        FacadeInlineNode::Strong(children) => {
            M11GfmInlineNode::Strong(children.into_iter().map(convert_node).collect())
        }
        FacadeInlineNode::Strikethrough(children) => {
            M11GfmInlineNode::Strikethrough(children.into_iter().map(convert_node).collect())
        }
        FacadeInlineNode::Link {
            destination,
            title,
            children,
        } => M11GfmInlineNode::Link {
            destination,
            title,
            children: children.into_iter().map(convert_node).collect(),
        },
        FacadeInlineNode::Image {
            destination,
            title,
            children,
        } => M11GfmInlineNode::Image {
            destination,
            title,
            children: children.into_iter().map(convert_node).collect(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complex_emphasis_links_and_raw_html_are_typed() {
        let nodes =
            project_m11_gfm_inline("*foo [bar](/url)* <x>", M11GfmInlineOptions::default(), &[])
                .expect("bounded exact projection");
        assert!(matches!(nodes[0], M11GfmInlineNode::Emphasis(_)));
        assert!(matches!(nodes[2], M11GfmInlineNode::Html(_)));
    }

    #[test]
    fn references_are_supplied_without_exposing_the_donor_map() {
        let nodes = project_m11_gfm_inline(
            "[label][ref]",
            M11GfmInlineOptions::default(),
            &[M11GfmInlineReference {
                normalized_label: "ref".into(),
                destination: "/uri".into(),
                title: "title".into(),
            }],
        )
        .expect("reference projection");
        assert!(matches!(nodes[0], M11GfmInlineNode::Link { .. }));
    }
}
