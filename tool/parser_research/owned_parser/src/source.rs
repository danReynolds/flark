//! Persistent UTF-8 source rope shared by the owned-parser trial.
//!
//! This implementation originated in Flark's parser research rather than in
//! Comrak. Leaves retain `Arc<str>` slices, so localized edits share unchanged
//! source bytes with prior revisions.

use std::sync::Arc;

#[derive(Clone, Default)]
pub struct SourceRope {
    root: Option<Arc<Node>>,
}

#[derive(Debug)]
enum Node {
    Leaf {
        source: Arc<str>,
        start: usize,
        len: usize,
        newlines: usize,
        hash32: u32,
        hash_power32: u32,
    },
    Branch {
        left: Arc<Node>,
        right: Arc<Node>,
        len: usize,
        newlines: usize,
        hash32: u32,
        hash_power32: u32,
        leaves: usize,
        height: usize,
    },
}

impl Node {
    fn len(&self) -> usize {
        match self {
            Self::Leaf { len, .. } | Self::Branch { len, .. } => *len,
        }
    }

    fn newlines(&self) -> usize {
        match self {
            Self::Leaf { newlines, .. } | Self::Branch { newlines, .. } => *newlines,
        }
    }

    fn hash32(&self) -> u32 {
        match self {
            Self::Leaf { hash32, .. } | Self::Branch { hash32, .. } => *hash32,
        }
    }

    fn hash_power32(&self) -> u32 {
        match self {
            Self::Leaf { hash_power32, .. } | Self::Branch { hash_power32, .. } => *hash_power32,
        }
    }

    fn leaves(&self) -> usize {
        match self {
            Self::Leaf { .. } => 1,
            Self::Branch { leaves, .. } => *leaves,
        }
    }

    fn height(&self) -> usize {
        match self {
            Self::Leaf { .. } => 1,
            Self::Branch { height, .. } => *height,
        }
    }
}

impl SourceRope {
    pub const CHUNK_SIZE: usize = 4096;

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(text: &str) -> Self {
        assert!(!text.contains('\r'), "source must be LF-normalized");
        if text.is_empty() {
            return Self::default();
        }
        let source: Arc<str> = Arc::from(text);
        let mut leaves = Vec::new();
        let mut start = 0;
        while start < source.len() {
            let mut end = (start + Self::CHUNK_SIZE).min(source.len());
            while end > start && !source.is_char_boundary(end) {
                end -= 1;
            }
            assert!(end > start);
            leaves.push(leaf(source.clone(), start, end - start));
            start = end;
        }
        fn build(nodes: &[Arc<Node>]) -> Option<Arc<Node>> {
            match nodes.len() {
                0 => None,
                1 => Some(nodes[0].clone()),
                len => {
                    let middle = len / 2;
                    Some(branch(
                        build(&nodes[..middle]).unwrap(),
                        build(&nodes[middle..]).unwrap(),
                    ))
                }
            }
        }
        Self {
            root: build(&leaves),
        }
    }

    pub fn len(&self) -> usize {
        self.root.as_deref().map_or(0, Node::len)
    }

    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    pub fn newline_count(&self) -> usize {
        self.root.as_deref().map_or(0, Node::newlines)
    }

    pub fn hash32(&self) -> u32 {
        self.root.as_deref().map_or(0, Node::hash32)
    }

    pub fn leaf_count(&self) -> usize {
        self.root.as_deref().map_or(0, Node::leaves)
    }

    pub fn height(&self) -> usize {
        self.root.as_deref().map_or(0, Node::height)
    }

    pub fn is_char_boundary(&self, offset: usize) -> bool {
        if offset == 0 || offset == self.len() {
            return true;
        }
        if offset > self.len() {
            return false;
        }
        fn visit(node: &Arc<Node>, offset: usize) -> bool {
            match node.as_ref() {
                Node::Leaf {
                    source, start, len, ..
                } => offset <= *len && source.is_char_boundary(start + offset),
                Node::Branch { left, right, .. } => {
                    if offset < left.len() {
                        visit(left, offset)
                    } else if offset == left.len() {
                        true
                    } else {
                        visit(right, offset - left.len())
                    }
                }
            }
        }
        visit(self.root.as_ref().unwrap(), offset)
    }

    pub fn replace(&self, start: usize, end: usize, replacement: &str) -> Self {
        assert!(start <= end && end <= self.len());
        assert!(self.is_char_boundary(start));
        assert!(self.is_char_boundary(end));
        assert!(!replacement.contains('\r'));
        let (prefix, rest) = split(self.root.clone(), start);
        let (_, suffix) = split(rest, end - start);
        Self {
            root: concat(concat(prefix, Self::from_str(replacement).root), suffix),
        }
    }

    pub fn slice(&self, start: usize, end: usize) -> String {
        assert!(start <= end && end <= self.len());
        fn write(node: &Arc<Node>, start: usize, end: usize, output: &mut String) {
            if start >= end {
                return;
            }
            match node.as_ref() {
                Node::Leaf {
                    source,
                    start: source_start,
                    ..
                } => output.push_str(&source[source_start + start..source_start + end]),
                Node::Branch { left, right, .. } => {
                    if start < left.len() {
                        write(left, start, end.min(left.len()), output);
                    }
                    if end > left.len() {
                        write(
                            right,
                            start.saturating_sub(left.len()),
                            end - left.len(),
                            output,
                        );
                    }
                }
            }
        }
        let mut output = String::with_capacity(end - start);
        if let Some(root) = &self.root {
            write(root, start, end, &mut output);
        }
        output
    }

    pub fn line_from(&self, offset: usize) -> (usize, String) {
        assert!(offset < self.len());
        let end = next_newline(&self.root, 0, offset).map_or(self.len(), |newline| newline + 1);
        (end, self.slice(offset, end))
    }

    pub fn materialize(&self) -> String {
        self.slice(0, self.len())
    }
}

fn leaf(source: Arc<str>, start: usize, len: usize) -> Arc<Node> {
    let bytes = &source.as_bytes()[start..start + len];
    let mut hash = 0u32;
    let mut power = 1u32;
    for byte in bytes {
        hash = hash
            .wrapping_mul(0x0010_0193)
            .wrapping_add(*byte as u32 + 1);
        power = power.wrapping_mul(0x0010_0193);
    }
    let newlines = bytes.iter().filter(|byte| **byte == b'\n').count();
    Arc::new(Node::Leaf {
        source,
        start,
        len,
        newlines,
        hash32: hash,
        hash_power32: power,
    })
}

fn branch(left: Arc<Node>, right: Arc<Node>) -> Arc<Node> {
    Arc::new(Node::Branch {
        len: left.len() + right.len(),
        newlines: left.newlines() + right.newlines(),
        hash32: left
            .hash32()
            .wrapping_mul(right.hash_power32())
            .wrapping_add(right.hash32()),
        hash_power32: left.hash_power32().wrapping_mul(right.hash_power32()),
        leaves: left.leaves() + right.leaves(),
        height: left.height().max(right.height()) + 1,
        left,
        right,
    })
}

fn split(node: Option<Arc<Node>>, offset: usize) -> (Option<Arc<Node>>, Option<Arc<Node>>) {
    let Some(node) = node else {
        assert_eq!(offset, 0);
        return (None, None);
    };
    assert!(offset <= node.len());
    if offset == 0 {
        return (None, Some(node));
    }
    if offset == node.len() {
        return (Some(node), None);
    }
    match node.as_ref() {
        Node::Leaf {
            source, start, len, ..
        } => (
            Some(leaf(source.clone(), *start, offset)),
            Some(leaf(source.clone(), start + offset, len - offset)),
        ),
        Node::Branch { left, right, .. } => {
            if offset < left.len() {
                let (prefix, rest) = split(Some(left.clone()), offset);
                (prefix, concat(rest, Some(right.clone())))
            } else if offset == left.len() {
                (Some(left.clone()), Some(right.clone()))
            } else {
                let (rest, suffix) = split(Some(right.clone()), offset - left.len());
                (concat(Some(left.clone()), rest), suffix)
            }
        }
    }
}

fn concat(left: Option<Arc<Node>>, right: Option<Arc<Node>>) -> Option<Arc<Node>> {
    let (left, right) = match (left, right) {
        (None, right) => return right,
        (left, None) => return left,
        (Some(left), Some(right)) => (left, right),
    };
    if left.height() > right.height() + 1 {
        let Node::Branch {
            left: outer_left,
            right: inner_left,
            ..
        } = left.as_ref()
        else {
            unreachable!()
        };
        return Some(balance(branch(
            outer_left.clone(),
            concat(Some(inner_left.clone()), Some(right)).unwrap(),
        )));
    }
    if right.height() > left.height() + 1 {
        let Node::Branch {
            left: inner_right,
            right: outer_right,
            ..
        } = right.as_ref()
        else {
            unreachable!()
        };
        return Some(balance(branch(
            concat(Some(left), Some(inner_right.clone())).unwrap(),
            outer_right.clone(),
        )));
    }
    Some(branch(left, right))
}

fn balance(node: Arc<Node>) -> Arc<Node> {
    let Node::Branch { left, right, .. } = node.as_ref() else {
        return node;
    };
    let balance = left.height() as isize - right.height() as isize;
    if balance > 1 {
        let Node::Branch {
            left: left_left,
            right: left_right,
            ..
        } = left.as_ref()
        else {
            unreachable!()
        };
        if left_left.height() < left_right.height() {
            let Node::Branch {
                left: pivot_left,
                right: pivot_right,
                ..
            } = left_right.as_ref()
            else {
                unreachable!()
            };
            return branch(
                branch(left_left.clone(), pivot_left.clone()),
                branch(pivot_right.clone(), right.clone()),
            );
        }
        return branch(left_left.clone(), branch(left_right.clone(), right.clone()));
    }
    if balance < -1 {
        let Node::Branch {
            left: right_left,
            right: right_right,
            ..
        } = right.as_ref()
        else {
            unreachable!()
        };
        if right_right.height() < right_left.height() {
            let Node::Branch {
                left: pivot_left,
                right: pivot_right,
                ..
            } = right_left.as_ref()
            else {
                unreachable!()
            };
            return branch(
                branch(left.clone(), pivot_left.clone()),
                branch(pivot_right.clone(), right_right.clone()),
            );
        }
        return branch(
            branch(left.clone(), right_left.clone()),
            right_right.clone(),
        );
    }
    node
}

fn next_newline(node: &Option<Arc<Node>>, base: usize, offset: usize) -> Option<usize> {
    let node = node.as_ref()?;
    if node.newlines() == 0 || base + node.len() <= offset {
        return None;
    }
    match node.as_ref() {
        Node::Leaf {
            source, start, len, ..
        } => {
            let local_start = offset.saturating_sub(base).min(*len);
            source.as_bytes()[start + local_start..start + len]
                .iter()
                .position(|byte| *byte == b'\n')
                .map(|relative| base + local_start + relative)
        }
        Node::Branch { left, right, .. } => next_newline(&Some(left.clone()), base, offset)
            .or_else(|| next_newline(&Some(right.clone()), base + left.len(), offset)),
    }
}
