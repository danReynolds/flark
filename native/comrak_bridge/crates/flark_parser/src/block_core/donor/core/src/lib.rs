//! Private, mechanically promoted snapshot of Flark's proven direct Comrak
//! block controller. Nothing in this crate is a production-facing parser API;
//! `flark_parser::block_core` adapts only the scalar command and source seams.

#![forbid(unsafe_code)]

mod parser;
mod reference_prefix;
mod source;
mod table;
mod tree;

pub use parser::{
    DIRECT_SOURCE_LINE_MAX_LEXICAL_SLACK, DIRECT_SOURCE_LINE_MAX_RETAINED_SOURCE_BYTES,
    DirectBlockKind, DirectClosedChild, DirectCommand, DirectCoveragePart, DirectExternalWork,
    DirectExternalWorkKind, DirectFenceCharacter, DirectFencedCodeBoundary,
    DirectFencedCodeCloseFacts, DirectFencedCodeFacts, DirectFinalFacts, DirectGrammarContinuation,
    DirectHeadingFacts, DirectItemFacts, DirectLeadingReferenceRemainderContinuation,
    DirectLineBoundaryDeferredRole, DirectLineBoundaryPairingView, DirectLineBoundaryPause,
    DirectLineBoundaryPauseCapture, DirectLineBoundaryPauseReceipt, DirectLineBoundaryResumeCursor,
    DirectLineEnding, DirectListFacts, DirectLogicalAction, DirectOwner, DirectParagraphOutcome,
    DirectPartialTab, DirectPollReceipt, DirectPollStatus, DirectReferencePrefixCommitStatus,
    DirectReferencePrefixContext, DirectReferencePrefixRequest, DirectRestartOutput,
    DirectSourceLinePollError, DirectSourceLinePollReceipt, DirectSourceLinePollStatus,
    DirectSourceLineSource, DirectSourceLineWork, DirectTerminatorResolution, DirectUnsupported,
    DirectValueBlockParser, ParseError,
};
pub use reference_prefix::{
    DirectReferenceDefinition, DirectReferenceLogicalPosition, DirectReferenceLogicalRange,
    DirectReferencePrefixDisposition, DirectReferencePrefixOutput, DirectReferencePrefixOutputAck,
    DirectReferencePrefixOutputAckStatus, DirectReferencePrefixPollError,
    DirectReferencePrefixPollReceipt, DirectReferencePrefixPollStatus, DirectReferencePrefixSource,
    DirectReferencePrefixTerminal, DirectReferencePrefixTerminalAck,
    DirectReferencePrefixTerminalOutput, DirectReferencePrefixWork, DirectReferenceValueTransform,
};
pub use tree::{ListDelimiter, ListType, SyntaxProfile};
