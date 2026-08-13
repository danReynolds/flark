/// Headless Dart API for Flark's bounded native Markdown runtime.
library;

export 'src/document.dart'
    show
        FlarkCoreAnchor,
        FlarkCoreDocument,
        FlarkCoreEditReceipt,
        FlarkCoreEditIntentDispositionV1,
        FlarkCoreEditPresentationTransitionV1,
        FlarkCoreEditIntentReceiptV1,
        FlarkCoreEditIntentTelemetryV1,
        FlarkCoreEditIntentV1,
        FlarkCoreHistoryDisposition,
        FlarkCoreHistoryToken,
        FlarkCoreNativeException,
        FlarkCoreSessionInspection,
        FlarkCoreWorkerException;
export 'src/editor_session.dart'
    show
        FlarkCoreAffinity,
        FlarkCoreEditorSession,
        FlarkCoreGraphemePolicy,
        FlarkCoreHistoryDropped,
        FlarkCoreHistoryOutcome,
        FlarkCoreHistoryReplayed,
        FlarkCoreSelectionSnapshot,
        FlarkCoreSemanticActionV1;
export 'src/models.dart'
    show
        FlarkCertification,
        FlarkCertificationRange,
        FlarkBlockQuotePresentation,
        FlarkCodeBlockPresentation,
        FlarkCodeBlockStyle,
        FlarkHeadingStyle,
        FlarkInlineFact,
        FlarkInlineFactKind,
        FlarkInlineContinuityPolicy,
        FlarkListItemPresentation,
        FlarkListMarkerStyle,
        FlarkProjectionSegment,
        FlarkSourceRange,
        FlarkTableAlignment,
        FlarkTableCellPresentation,
        FlarkTablePresentation,
        FlarkViewport,
        FlarkViewportRow,
        FlarkViewportRowContinuityPolicy,
        FlarkViewportRowEditCapability;
export 'src/projection_continuity.dart'
    show
        FlarkProjectionContinuityReceipt,
        authorizeInlineProjectionContinuity,
        authorizeRowProjectionContinuity,
        authorizeTableCellProjectionContinuity;
export 'src/presentation.dart'
    show
        FlarkCoreCommittedPresentationGapV1,
        FlarkCoreCommittedPresentationSurfaceV1,
        FlarkCoreCommittedPresentationTransitionV1,
        FlarkCorePresentationInlineStyle,
        FlarkCorePresentationRow,
        FlarkCorePresentationRun,
        mapCommittedPresentationSurfacesThroughLiteralSpliceV1,
        resolveCommittedPresentationTransitionV1;
export 'src/native/native_document.dart'
    show FlarkNativeDocument, FlarkNativeException;
