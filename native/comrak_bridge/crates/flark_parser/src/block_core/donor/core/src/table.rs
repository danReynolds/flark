//! Deliberately unpromoted GFM table seam in the direct production parser.
//!
//! The direct controller now preserves selected GFM identity, but tables need
//! a bounded variable-cell command protocol and recursive-Green schema before
//! they can become authoritative. Until that protocol is present, this seam
//! leaves the source as ordinary Paragraph content instead of importing the
//! research-only AST renderer or fabricating table authority.

use crate::parser::{ParseError, ValueBlockParser};
use crate::tree::NodeId;

pub(crate) struct TableOpening {
    pub(crate) container: NodeId,
    pub(crate) replace: bool,
    pub(crate) mark_visited: bool,
}

pub(crate) fn try_opening_block(
    _parser: &mut ValueBlockParser,
    _container: NodeId,
    _line: &str,
) -> Result<Option<TableOpening>, ParseError> {
    Ok(None)
}

pub(crate) fn matches(_line: &str) -> Result<bool, ParseError> {
    Ok(false)
}
