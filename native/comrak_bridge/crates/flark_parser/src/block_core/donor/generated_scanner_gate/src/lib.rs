//! Gate for deriving a resumable scanner from the same pinned rule text as
//! Comrak's ordinary scanner.

mod atx_cursor_generated;
mod atx_fused_cursor;
mod atx_generated;
mod atx_tail_cursor;
mod open_code_fence_cursor_generated;

pub use atx_cursor_generated::{
    AtxCursorSource, CursorAtxScanner, CursorScanError, CursorScanReceipt, CursorScanResult,
    CURSOR_ATX_MAX_LOOKAHEAD_SLACK, CURSOR_ATX_REJECTION_PREFIX_CAP,
};
pub use atx_fused_cursor::{
    FusedAtxDonorMatch, FusedAtxLineScanError, FusedAtxLineScanReceipt, FusedAtxLineScanResult,
    FusedAtxLineScanner, FusedAtxLineSource, FUSED_ATX_MAX_SOURCE_ACCESSES_PER_POLL,
    FUSED_ATX_REJECTION_PREFIX_CAP,
};
pub use atx_generated::{AtxScanner, ScanResult};
pub use atx_tail_cursor::{
    AtxLineCuts, AtxLineCutsError, AtxTailCursorScanner, AtxTailCursorSource, AtxTailCuts,
    AtxTailScanError, AtxTailScanReceipt, AtxTailScanResult,
};
pub use open_code_fence_cursor_generated::{
    OpenCodeFenceCursorScanner, OpenCodeFenceCursorSource, OpenCodeFenceScanError,
    OpenCodeFenceScanReceipt, OpenCodeFenceScanResult,
};
