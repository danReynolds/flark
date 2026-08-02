/// Pure-Dart Flark v3 engine preview.
///
/// The v3 API is still being completed and is not yet the default
/// `package:flark/flark.dart` surface. It is intentionally available as a
/// Dart library now so non-Flutter consumers and official adapters exercise
/// the same document-session authority throughout production implementation.
///
/// {@canonicalFor flark_v3_source_document.FlarkV3RevisionMismatch}
/// {@canonicalFor flark_v3_source_document.FlarkV3SourceBulkOperationRequired}
/// {@canonicalFor flark_v3_source_document.FlarkV3SourceEdit}
/// {@canonicalFor flark_v3_source_document.FlarkV3SourceTransaction}
library;

export 'src/v3/runtime/public/flark_v3_document_query.dart'
    show
        FlarkV3AtxHeadingFacts,
        FlarkV3BlockQuoteFacts,
        FlarkV3BulletListFacts,
        FlarkV3BulletListMarker,
        FlarkV3DocumentBlockRangeBudget,
        FlarkV3DocumentBlockRangeContinuation,
        FlarkV3DocumentBlockRangeResult,
        FlarkV3CodeFenceMarker,
        FlarkV3DocumentPointPath,
        FlarkV3DocumentPointPathNode,
        FlarkV3DocumentPointPathNodeKind,
        FlarkV3DocumentPendingQuery,
        FlarkV3DocumentPendingReason,
        FlarkV3DocumentProjection,
        FlarkV3DocumentQueryAffinity,
        FlarkV3DocumentQueryBudget,
        FlarkV3DocumentQueryException,
        FlarkV3DocumentQueryGapReason,
        FlarkV3DocumentQueryResult,
        FlarkV3DocumentSourceGapQuery,
        FlarkV3DocumentSourceGapBlockRange,
        FlarkV3DocumentStructuralBlock,
        FlarkV3DocumentStructuralBlockRange,
        FlarkV3DocumentStructuralQuery,
        FlarkV3DocumentStructure,
        FlarkV3DocumentStructureKind,
        FlarkV3DocumentUnknownReason,
        FlarkV3FencedCodeFacts,
        FlarkV3HeadingFacts,
        FlarkV3IndentedCodeFacts,
        FlarkV3OrderedListDelimiter,
        FlarkV3OrderedListFacts,
        FlarkV3RecursiveGreenAncestor,
        FlarkV3RecursiveGreenCoveragePart,
        FlarkV3RecursiveGreenKind,
        FlarkV3RecursiveGreenLogicalAtom,
        FlarkV3RecursiveGreenLogicalAtomKind,
        FlarkV3RecursiveGreenCodePathFact,
        FlarkV3RecursiveGreenHeadingPathFact,
        FlarkV3RecursiveGreenHeadingStyle,
        FlarkV3RecursiveGreenHtmlPathFact,
        FlarkV3RecursiveGreenItemPathFact,
        FlarkV3RecursiveGreenListPathFact,
        FlarkV3RecursiveGreenListStyle,
        FlarkV3RecursiveGreenPathFact,
        FlarkV3RecursiveGreenPointQuery,
        FlarkV3RecursiveGreenQueryWork,
        FlarkV3RecursiveGreenRenderableRow,
        FlarkV3RecursiveGreenRowPathFrame,
        FlarkV3RecursiveGreenRowPresentationKind,
        FlarkV3RecursiveGreenRowEditCapability,
        FlarkV3RecursiveGreenRowRange,
        FlarkV3SetextHeadingFacts,
        FlarkV3DocumentPendingBlockRange,
        FlarkV3SourceSpan,
        FlarkV3ThematicBreakFacts,
        FlarkV3ThematicBreakMarker;
export 'src/v3/runtime/public/flark_v3_block_quote_projection.dart'
    show
        FlarkV3BlockQuoteLineProjectionKind,
        FlarkV3BlockQuoteLineProjectionRecord,
        FlarkV3BlockQuoteProjectionPayload;
export 'src/v3/runtime/public/flark_v3_bullet_list_projection.dart'
    show
        FlarkV3BulletListItemProjectionRecord,
        FlarkV3BulletListProjectionPayload,
        FlarkV3TightListItemEditingInputs,
        FlarkV3TightListItemProjectionPayload,
        FlarkV3TightListItemProjectionRecord,
        FlarkV3TightBulletListItemEditingInputs;
export 'src/v3/runtime/public/flark_v3_ordered_list_projection.dart'
    show
        FlarkV3OrderedListContinuationOverflowPolicy,
        FlarkV3OrderedListItemProjectionRecord,
        FlarkV3OrderedListProjectionDecodeException,
        FlarkV3OrderedListProjectionDecoder,
        FlarkV3OrderedListProjectionPayload;
export 'src/v3/runtime/public/flark_v3_document_runtime.dart'
    show
        FlarkV3DocumentEditResult,
        FlarkV3DocumentOrdinalWindowBudget,
        FlarkV3DocumentOrdinalWindowDemand,
        FlarkV3DocumentOrdinalWindowFailureReason,
        FlarkV3DocumentOrdinalWindowResult,
        FlarkV3DocumentRuntime,
        FlarkV3DocumentRuntimeState,
        FlarkV3DocumentRuntimeStatus,
        FlarkV3RuntimeClosedBeforeReady,
        FlarkV3RuntimeParserFailure,
        FlarkV3RuntimePlatformSupport,
        FlarkV3RuntimeUnavailable,
        FlarkV3ExactDocumentOrdinalWindow,
        FlarkV3UnavailableDocumentOrdinalWindow;
export 'src/v3/runtime/public/flark_v3_indented_code_projection.dart'
    show
        FlarkV3IndentedCodeLineProjectionRecord,
        FlarkV3IndentedCodeProjectionPayload;
export 'src/v3/runtime/public/flark_v3_inline_facts.dart'
    show
        FlarkV3InlineFact,
        FlarkV3InlineFactKind,
        FlarkV3InlineFacts,
        FlarkV3InlineFactsDisposition,
        FlarkV3InlineImageAnnotation,
        FlarkV3InlineLinkAnnotation,
        FlarkV3InlineLinkKind,
        FlarkV3InlineLinkTargetRecipe;
export 'src/v3/runtime/public/flark_v3_runtime_assets.dart'
    show FlarkV3WebRuntimeAssets;
export 'src/v3/runtime/public/flark_v3_visible_block_set.dart'
    show
        FlarkV3ExactVisibleBlockSet,
        FlarkV3PendingVisibleBlockSet,
        FlarkV3SourceGapVisibleBlockSet,
        FlarkV3VisibleBlockDemand,
        FlarkV3VisibleBlockSet,
        FlarkV3VisibleBlockSetMaterializer;
export 'src/v3/source/flark_v3_source_document.dart'
    show
        FlarkV3RevisionMismatch,
        FlarkV3SourceBulkOperationRequired,
        FlarkV3SourceEdit,
        FlarkV3SourceTransaction;
