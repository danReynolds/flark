/// Platform-neutral implementation surface shared with official Flark
/// adapters.
///
/// Applications should import `package:flark/flark.dart`. This library exists
/// so adapters such as `flark_flutter` do not reach through another package's
/// `src/` directory or acquire a second Markdown implementation.
library;

export 'src/v2/core/core.dart';
export 'src/v2/markdown/markdown.dart';
export 'src/v2/markdown/inline/flark_inline_delimiter_placement.dart';
export 'src/v2/markdown/inline/flark_inline_run_scanner.dart';
export 'src/v2/markdown/source/flark_markdown_fenced_code_policy.dart';
export 'src/v2/markdown/source/flark_markdown_fenced_code_scanner.dart';
export 'src/v2/markdown/source/flark_markdown_input_engine.dart';
export 'src/v2/native/native.dart';
export 'src/v2/projection/projection.dart';
export 'src/v2/render_plan/render_plan.dart';
export 'src/v3/host/host.dart';
export 'src/v3/editor/flark_v3_inline_island_presentation.dart'
    show
        FlarkV3AuthoritativeInlineIslandPresentation,
        FlarkV3InlineIslandPresentation,
        FlarkV3InlineIslandSourcePaintReason,
        FlarkV3SourcePaintInlineIslandPresentation;
export 'src/v3/editor/flark_v3_inline_projection.dart'
    show
        FlarkV3InlineDisplayRun,
        FlarkV3InlineDelimiterPair,
        FlarkV3InlineDelimiterTopology,
        FlarkV3InlineDeletionPlan,
        FlarkV3InlineEditPlan,
        FlarkV3InlineMarkerPolicy,
        FlarkV3InlineProjection,
        FlarkV3InlineProjectionAffinity,
        FlarkV3InlineProjectionException,
        FlarkV3InlineProjectionWorkReceipt,
        FlarkV3InlineSemanticStack,
        FlarkV3InlineUtf16Range;
export 'src/v3/editor/flark_v3_source_projection.dart'
    show
        FlarkV3SourceBackedProjectionEditPolicy,
        FlarkV3SourceProjection,
        FlarkV3SourceProjectionAffinity,
        FlarkV3SourceProjectionDisplayEdit,
        FlarkV3SourceProjectionEditPlan,
        FlarkV3SourceProjectionEditPolicy,
        FlarkV3SourceProjectionEditRequest,
        FlarkV3SourceProjectionPiece,
        FlarkV3SourceProjectionPieceKind,
        FlarkV3SourceProjectionReplacement;
export 'src/v3/runtime/public/flark_v3_checkpoint_b_probe.dart'
    show runFlarkV3CheckpointBProbeJson;
export 'src/v3/runtime/public/flark_v3_document_query.dart'
    show
        FlarkV3AtxHeadingFacts,
        FlarkV3BlockQuoteFacts,
        FlarkV3BulletListFacts,
        FlarkV3BulletListMarker,
        FlarkV3CodeFenceMarker,
        FlarkV3DocumentBlockRangeBudget,
        FlarkV3DocumentBlockRangeContinuation,
        FlarkV3DocumentBlockRangeResult,
        FlarkV3DocumentPointPath,
        FlarkV3DocumentPointPathNode,
        FlarkV3DocumentPointPathNodeKind,
        FlarkV3DocumentPendingBlockRange,
        FlarkV3DocumentPendingQuery,
        FlarkV3DocumentPendingReason,
        FlarkV3DocumentProjection,
        FlarkV3DocumentQueryAffinity,
        FlarkV3DocumentQueryBudget,
        FlarkV3DocumentQueryException,
        FlarkV3DocumentQueryGapReason,
        FlarkV3DocumentQueryResult,
        FlarkV3DocumentSourceGapBlockRange,
        FlarkV3DocumentSourceGapQuery,
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
export 'src/v3/runtime/flark_v3_parser_transport.dart'
    show FlarkV3ParserSessionBinding;
export 'src/v3/runtime/public/flark_v3_document_runtime.dart'
    show
        FlarkV3DocumentOrdinalWindowBudget,
        FlarkV3DocumentOrdinalWindowDemand,
        FlarkV3DocumentOrdinalWindowFailureReason,
        FlarkV3DocumentOrdinalWindowResult,
        FlarkV3DocumentRuntimeAdapter,
        FlarkV3DocumentRuntimeAdapterLease,
        FlarkV3ExactDocumentOrdinalWindow,
        FlarkV3InlineDemandDisposition,
        FlarkV3LeafProjectionDemandDisposition,
        FlarkV3ViewportPresentationDemand,
        FlarkV3ViewportPresentationDemandDisposition,
        FlarkV3ViewportPresentationDemandReceipt,
        FlarkV3ViewportPresentationPageResult,
        FlarkV3ExactViewportPresentationPage,
        FlarkV3UnavailableDocumentOrdinalWindow,
        FlarkV3UnavailableViewportPresentationPage,
        FlarkV3ViewportPresentationUnavailableReason;
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
export 'src/v3/runtime/public/flark_v3_indented_code_projection.dart'
    show
        FlarkV3IndentedCodeLineProjectionRecord,
        FlarkV3IndentedCodeProjectionPayload;
export 'src/v3/runtime/public/flark_v3_viewport_page_materializer.dart';
export 'src/v3/runtime/public/flark_v3_visible_block_set.dart'
    show
        FlarkV3ExactVisibleBlockSet,
        FlarkV3PendingVisibleBlockSet,
        FlarkV3SourceGapVisibleBlockSet,
        FlarkV3VisibleBlockDemand,
        FlarkV3VisibleBlockSet,
        FlarkV3VisibleBlockSetMaterializer;
export 'src/v3/session/session.dart';
export 'src/v3/source/source.dart';
