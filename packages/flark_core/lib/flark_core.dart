/// Headless Dart API for Flark's bounded native Markdown runtime.
library;

export 'src/document.dart'
    show
        FlarkCoreAnchor,
        FlarkCoreDocument,
        FlarkCoreEditReceipt,
        FlarkCoreEditIntentDispositionV1,
        FlarkCoreEditIntentReceiptV1,
        FlarkCoreEditIntentV1,
        FlarkCoreHistoryDisposition,
        FlarkCoreHistoryToken,
        FlarkCoreNativeException,
        FlarkCoreSessionInspection;
export 'src/editor_session.dart'
    show
        FlarkCoreAffinity,
        FlarkCoreEditorSession,
        FlarkCoreGraphemePolicy,
        FlarkCoreHistoryDropped,
        FlarkCoreHistoryOutcome,
        FlarkCoreHistoryReplayed,
        FlarkCoreSelectionSnapshot;
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
        authorizeRowProjectionContinuity;
export 'src/native/native_document.dart'
    show FlarkNativeDocument, FlarkNativeException;
