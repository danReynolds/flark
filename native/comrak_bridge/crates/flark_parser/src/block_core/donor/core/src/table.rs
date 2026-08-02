//! Deliberately unreachable GFM table seam in the private CommonMark-only
//! production promotion.
//!
//! The promoted direct controller rejects every non-CommonMark profile at
//! construction. Retaining a table implementation here would require the
//! research-only oracle facade and would broaden the production dependency
//! surface without making any reachable direct transition more exact.

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
