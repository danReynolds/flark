//! Disposable native/WASM ABI probe for a revisioned persistent source rope.
//!
//! This validates the cross-runtime edit contract only. The instrumented
//! Comrak patch in this directory validates checkpoint/resume and syntax-tree
//! reuse separately.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use comrak::{parse_document, Arena, Options};

const HASH_BASE: u32 = 0x0010_0193;
const CHUNK_SIZE: usize = 4096;

pub const STATUS_OK: u32 = 0;
pub const STATUS_NULL: u32 = 1;
pub const STATUS_REVISION: u32 = 2;
pub const STATUS_HASH: u32 = 3;
pub const STATUS_RANGE: u32 = 4;
pub const STATUS_UTF8: u32 = 5;
pub const STATUS_PANIC: u32 = 6;

pub struct PrototypeDocumentHandle {
    revision: u32,
    text: ByteRope,
}

#[derive(Clone, Default)]
struct ByteRope {
    root: Option<Arc<Node>>,
}

enum Node {
    Leaf {
        source: Arc<[u8]>,
        start: usize,
        len: usize,
        hash32: u32,
        hash_power32: u32,
    },
    Branch {
        left: Arc<Node>,
        right: Arc<Node>,
        len: usize,
        hash32: u32,
        hash_power32: u32,
        height: usize,
    },
}

impl Node {
    fn len(&self) -> usize {
        match self {
            Self::Leaf { len, .. } | Self::Branch { len, .. } => *len,
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

    fn height(&self) -> usize {
        match self {
            Self::Leaf { .. } => 1,
            Self::Branch { height, .. } => *height,
        }
    }
}

impl ByteRope {
    fn from_str(text: &str) -> Self {
        if text.is_empty() {
            return Self::default();
        }
        let source: Arc<[u8]> = Arc::from(text.as_bytes());
        let mut leaves = Vec::new();
        let mut start = 0;
        while start < source.len() {
            let mut end = (start + CHUNK_SIZE).min(source.len());
            while end > start && end < source.len() && is_continuation(source[end]) {
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

    fn len(&self) -> usize {
        self.root.as_deref().map_or(0, Node::len)
    }

    fn hash32(&self) -> u32 {
        self.root.as_deref().map_or(0, Node::hash32)
    }

    fn is_char_boundary(&self, offset: usize) -> bool {
        if offset == 0 || offset == self.len() {
            return true;
        }
        if offset > self.len() {
            return false;
        }
        !is_continuation(self.byte_at(offset))
    }

    fn byte_at(&self, offset: usize) -> u8 {
        assert!(offset < self.len());
        fn visit(node: &Arc<Node>, offset: usize) -> u8 {
            match node.as_ref() {
                Node::Leaf { source, start, .. } => source[start + offset],
                Node::Branch { left, right, .. } => {
                    if offset < left.len() {
                        visit(left, offset)
                    } else {
                        visit(right, offset - left.len())
                    }
                }
            }
        }
        visit(self.root.as_ref().unwrap(), offset)
    }

    fn replace(&self, start: usize, end: usize, replacement: &str) -> Self {
        assert!(start <= end && end <= self.len());
        assert!(self.is_char_boundary(start));
        assert!(self.is_char_boundary(end));
        let (prefix, rest) = split(self.root.clone(), start);
        let (_, suffix) = split(rest, end - start);
        Self {
            root: concat(concat(prefix, Self::from_str(replacement).root), suffix),
        }
    }

    fn materialize(&self) -> Vec<u8> {
        fn write(node: &Arc<Node>, output: &mut Vec<u8>) {
            match node.as_ref() {
                Node::Leaf {
                    source, start, len, ..
                } => output.extend_from_slice(&source[*start..*start + *len]),
                Node::Branch { left, right, .. } => {
                    write(left, output);
                    write(right, output);
                }
            }
        }
        let mut output = Vec::with_capacity(self.len());
        if let Some(root) = &self.root {
            write(root, &mut output);
        }
        output
    }
}

fn is_continuation(byte: u8) -> bool {
    byte & 0b1100_0000 == 0b1000_0000
}

fn leaf(source: Arc<[u8]>, start: usize, len: usize) -> Arc<Node> {
    let mut hash32 = 0u32;
    let mut hash_power32 = 1u32;
    for byte in &source[start..start + len] {
        hash32 = hash32
            .wrapping_mul(HASH_BASE)
            .wrapping_add(*byte as u32 + 1);
        hash_power32 = hash_power32.wrapping_mul(HASH_BASE);
    }
    Arc::new(Node::Leaf {
        source,
        start,
        len,
        hash32,
        hash_power32,
    })
}

fn branch(left: Arc<Node>, right: Arc<Node>) -> Arc<Node> {
    Arc::new(Node::Branch {
        len: left.len() + right.len(),
        hash32: left
            .hash32()
            .wrapping_mul(right.hash_power32())
            .wrapping_add(right.hash32()),
        hash_power32: left.hash_power32().wrapping_mul(right.hash_power32()),
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

#[no_mangle]
pub extern "C" fn flark_v3_input_alloc(len: u32) -> *mut u8 {
    if len == 0 {
        return std::ptr::null_mut();
    }
    let mut bytes = Vec::<u8>::with_capacity(len as usize);
    let pointer = bytes.as_mut_ptr();
    std::mem::forget(bytes);
    pointer
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)] // Safety contract is expressed by the C ABI.
pub extern "C" fn flark_v3_input_free(pointer: *mut u8, len: u32) {
    if pointer.is_null() || len == 0 {
        return;
    }
    // SAFETY: the pointer and capacity originate from flark_v3_input_alloc.
    unsafe {
        let _ = Vec::from_raw_parts(pointer, 0, len as usize);
    }
}

#[no_mangle]
pub extern "C" fn flark_v3_document_new(
    pointer: *const u8,
    len: u32,
) -> *mut PrototypeDocumentHandle {
    catch_unwind(AssertUnwindSafe(|| {
        let bytes = match input_slice(pointer, len) {
            Some(bytes) => bytes,
            None => return std::ptr::null_mut(),
        };
        let Ok(text) = std::str::from_utf8(bytes) else {
            return std::ptr::null_mut();
        };
        Box::into_raw(Box::new(PrototypeDocumentHandle {
            revision: 0,
            text: ByteRope::from_str(text),
        }))
    }))
    .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)] // Safety contract is expressed by the C ABI.
pub extern "C" fn flark_v3_document_free(handle: *mut PrototypeDocumentHandle) {
    if handle.is_null() {
        return;
    }
    // SAFETY: the pointer originates from flark_v3_document_new and ownership
    // is transferred exactly once to this function.
    unsafe {
        let _ = Box::from_raw(handle);
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)] // Safety contract is expressed by the C ABI.
pub extern "C" fn flark_v3_document_apply(
    handle: *mut PrototypeDocumentHandle,
    base_revision: u32,
    before_hash32: u32,
    start_utf8: u32,
    end_utf8: u32,
    replacement_pointer: *const u8,
    replacement_len: u32,
) -> u32 {
    catch_unwind(AssertUnwindSafe(|| {
        if handle.is_null() {
            return STATUS_NULL;
        }
        let replacement_bytes = match input_slice(replacement_pointer, replacement_len) {
            Some(bytes) => bytes,
            None => return STATUS_NULL,
        };
        let Ok(replacement) = std::str::from_utf8(replacement_bytes) else {
            return STATUS_UTF8;
        };
        // SAFETY: non-null handle is owned by the caller for this invocation.
        let document = unsafe { &mut *handle };
        if document.revision != base_revision {
            return STATUS_REVISION;
        }
        if document.text.hash32() != before_hash32 {
            return STATUS_HASH;
        }
        let start = start_utf8 as usize;
        let end = end_utf8 as usize;
        if start > end || end > document.text.len() {
            return STATUS_RANGE;
        }
        if !document.text.is_char_boundary(start) || !document.text.is_char_boundary(end) {
            return STATUS_UTF8;
        }
        let Some(revision) = document.revision.checked_add(1) else {
            return STATUS_RANGE;
        };
        document.text = document.text.replace(start, end, replacement);
        document.revision = revision;
        STATUS_OK
    }))
    .unwrap_or(STATUS_PANIC)
}

#[no_mangle]
pub extern "C" fn flark_v3_document_revision(handle: *const PrototypeDocumentHandle) -> u32 {
    handle_ref(handle).map_or(0, |document| document.revision)
}

#[no_mangle]
pub extern "C" fn flark_v3_document_hash32(handle: *const PrototypeDocumentHandle) -> u32 {
    handle_ref(handle).map_or(0, |document| document.text.hash32())
}

#[no_mangle]
pub extern "C" fn flark_v3_document_len(handle: *const PrototypeDocumentHandle) -> u32 {
    handle_ref(handle)
        .and_then(|document| document.text.len().try_into().ok())
        .unwrap_or(0)
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)] // Safety contract is expressed by the C ABI.
pub extern "C" fn flark_v3_document_copy_text(
    handle: *const PrototypeDocumentHandle,
    output: *mut u8,
    capacity: u32,
) -> u32 {
    let Some(document) = handle_ref(handle) else {
        return 0;
    };
    let bytes = document.text.materialize();
    if bytes.len() > capacity as usize || (output.is_null() && !bytes.is_empty()) {
        return 0;
    }
    if !bytes.is_empty() {
        // SAFETY: caller promises a writable buffer of at least capacity bytes.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), output, bytes.len());
        }
    }
    bytes.len().try_into().unwrap_or(0)
}

/// Measures the real Comrak work represented by one locally invalidated leaf.
///
/// This is not the resumable parser export: the fork patch validates restored
/// state and suffix reuse. It keeps the WASM performance receipt honest by
/// running stock Comrak's complete block + inline pipeline over the bounded
/// payload that the checkpoint algorithm actually reparses.
#[no_mangle]
pub extern "C" fn flark_v3_comrak_parse_fragment(pointer: *const u8, len: u32) -> u32 {
    catch_unwind(AssertUnwindSafe(|| {
        let Some(bytes) = input_slice(pointer, len) else {
            return 0;
        };
        let Ok(text) = std::str::from_utf8(bytes) else {
            return 0;
        };
        let mut options = Options::default();
        options.extension.autolink = true;
        options.extension.strikethrough = true;
        options.extension.table = true;
        options.extension.tagfilter = true;
        options.extension.tasklist = true;
        let arena = Arena::new();
        let root = parse_document(&arena, text, &options);
        root.descendants().count().try_into().unwrap_or(u32::MAX)
    }))
    .unwrap_or(0)
}

fn input_slice<'a>(pointer: *const u8, len: u32) -> Option<&'a [u8]> {
    if len == 0 {
        return Some(&[]);
    }
    if pointer.is_null() {
        return None;
    }
    // SAFETY: the ABI caller promises a readable input allocation of len bytes.
    Some(unsafe { std::slice::from_raw_parts(pointer, len as usize) })
}

fn handle_ref<'a>(handle: *const PrototypeDocumentHandle) -> Option<&'a PrototypeDocumentHandle> {
    if handle.is_null() {
        None
    } else {
        // SAFETY: the non-null pointer originates from flark_v3_document_new.
        Some(unsafe { &*handle })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn byte_hash(text: &str) -> u32 {
        text.bytes().fold(0u32, |hash, byte| {
            hash.wrapping_mul(HASH_BASE).wrapping_add(byte as u32 + 1)
        })
    }

    #[test]
    fn persistent_rope_matches_a_unicode_string_oracle() {
        assert_eq!(byte_hash("a😀 café β\n"), 0xb991_edd9);
        let mut oracle = "alpha 😀 café\nβeta\n".repeat(20_000);
        let mut rope = ByteRope::from_str(&oracle);
        for iteration in 0..10_000 {
            let mut start = 16 + (iteration * 7919) % (oracle.len() - 32);
            while !oracle.is_char_boundary(start) {
                start += 1;
            }
            let replacement = match iteration % 5 {
                0 => "x",
                1 => "é",
                2 => "😀",
                3 => "",
                _ => "\n",
            };
            let end = if iteration % 3 == 0 {
                oracle[start..]
                    .chars()
                    .next()
                    .map_or(start, |character| start + character.len_utf8())
            } else {
                start
            };
            rope = rope.replace(start, end, replacement);
            oracle.replace_range(start..end, replacement);
            if iteration % 250 == 0 || iteration == 9_999 {
                assert_eq!(rope.materialize(), oracle.as_bytes());
                assert_eq!(rope.hash32(), byte_hash(&oracle));
            }
        }
    }

    #[test]
    fn abi_rejects_stale_hash_revision_and_invalid_utf8_boundaries() {
        let source = "a😀b";
        let handle = flark_v3_document_new(source.as_ptr(), source.len() as u32);
        assert!(!handle.is_null());
        let hash = flark_v3_document_hash32(handle);
        assert_eq!(
            flark_v3_document_apply(handle, 1, hash, 1, 1, b"x".as_ptr(), 1),
            STATUS_REVISION,
        );
        assert_eq!(
            flark_v3_document_apply(handle, 0, hash ^ 1, 1, 1, b"x".as_ptr(), 1),
            STATUS_HASH,
        );
        assert_eq!(
            flark_v3_document_apply(handle, 0, hash, 2, 2, b"x".as_ptr(), 1),
            STATUS_UTF8,
        );
        assert_eq!(
            flark_v3_document_apply(handle, 0, hash, 1, 1, "é".as_ptr(), 2),
            STATUS_OK,
        );
        assert_eq!(flark_v3_document_revision(handle), 1);
        assert_eq!(flark_v3_document_len(handle), (source.len() + 2) as u32);
        flark_v3_document_free(handle);
    }
}
