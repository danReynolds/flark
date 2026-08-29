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
        FlarkCoreInlineContinuationRecipeV1,
        FlarkCoreInlineContinuationScalarPolicyV1,
        FlarkCoreHistoryDisposition,
        FlarkCoreHistoryToken,
        FlarkCoreNativeException,
        FlarkCoreSessionInspection,
        FlarkCoreSourceTransactionReceiptV1,
        FlarkCoreWorkerException;
export 'src/editor_session.dart'
    show
        FlarkCoreAffinity,
        FlarkCoreEditorSession,
        FlarkCoreEditIntentOutcomeV1,
        FlarkCoreGraphemePolicy,
        FlarkCoreHistoryDropped,
        FlarkCoreHistoryOutcome,
        FlarkCoreHistoryReplayed,
        FlarkCoreInlineContinuationRewriteV1,
        FlarkCoreInlineContinuationV1,
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
        FlarkLiteralEditClass,
        FlarkLiteralSafeEnvelope,
        FlarkListItemPresentation,
        FlarkListMarkerStyle,
        FlarkPendingPresentationPlan,
        FlarkPendingPresentationStep,
        FlarkProjectionSegment,
        FlarkProjectionEditCell,
        FlarkProjectionEditMatcher,
        FlarkProjectionResultBlockKind,
        FlarkProjectionResultBlockShell,
        FlarkSemanticTarget,
        FlarkSemanticTargetKind,
        FlarkSemanticTargetSyntax,
        FlarkSourceRange,
        FlarkTableAlignment,
        FlarkTableCellPresentation,
        FlarkTablePresentation,
        FlarkViewport,
        FlarkViewportRow,
        FlarkViewportRowEditCapability;
export 'src/editor_coordinator.dart'
    show
        FlarkEditorCoordinator,
        FlarkEditorCommandKind,
        FlarkEditorCommandTicket,
        FlarkEditorStamp,
        FlarkEditorStatus,
        FlarkPublicationAwaitingCertification,
        FlarkPublicationIdle,
        FlarkPublicationPhase;
export 'src/editor_snapshot.dart'
    show FlarkEditorSnapshot, FlarkEditorSnapshotRow;
export 'src/editor_text.dart'
    show
        FlarkEditorInputValue,
        FlarkTextAffinity,
        FlarkTextRange,
        FlarkTextSelection;
export 'src/pending_presentation_evolution.dart'
    show
        advancePendingDependencyPresentation,
        advancePendingPresentationRow,
        bindPendingDependencyPresentation;
export 'src/optimistic_range_map.dart'
    show FlarkOptimisticRangeMap, FlarkOptimisticViewportEdit;
export 'src/surface_projection.dart'
    show
        FlarkSurfaceInlineStyle,
        FlarkSurfaceProjection,
        FlarkSurfaceRow,
        FlarkSurfaceTextRun;
export 'src/surface_projector.dart' show FlarkSurfaceProjector;
export 'src/viewport_installation.dart' show FlarkViewportInstallationPlan;
export 'src/viewport_navigation.dart'
    show
        FlarkViewportNavigationState,
        FlarkViewportPageAnchor,
        FlarkViewportQueryPage;
export 'src/projection_continuity.dart'
    show
        FlarkPendingDependencyAuthority,
        FlarkBoundedPendingPresentationPlanReceipt,
        FlarkProjectionContinuityReceipt,
        FlarkProjectionEditCellReceipt,
        bindPendingDependencyAuthority,
        authorizeBoundedPendingPresentationPlan,
        authorizeProjectionEditCell,
        authorizeRowProjectionContinuity;
export 'src/pending_presentation.dart'
    show
        FlarkPendingCaretBoundary,
        FlarkPendingDependencyPresentation,
        materializeBoundedPendingPresentationPlan,
        FlarkPendingPresentationAdoption,
        FlarkPendingPresentationSnapshot,
        FlarkPendingPresentationPart,
        FlarkPendingStructuralSurface;
export 'src/presentation.dart'
    show
        FlarkCoreCommittedPresentationGapV1,
        FlarkCoreCommittedPresentationSurfaceRole,
        FlarkCoreCommittedPresentationSurfaceV1,
        FlarkCoreCommittedPresentationTransitionV1,
        FlarkCorePresentationInlineStyle,
        FlarkCorePresentationRow,
        FlarkCorePresentationRun,
        resolveCommittedPresentationTransitionV1;
export 'src/native/native_document.dart'
    show
        FlarkNativeDocument,
        FlarkNativeEditIntentDispositionV1,
        FlarkNativeEditIntentReceiptV1,
        FlarkNativeEditIntentV1,
        FlarkNativeEditPresentationTransitionV1,
        FlarkNativeEditReceipt,
        FlarkNativeException,
        FlarkNativeGlobalLiveStateInspection,
        FlarkNativeHistoryDisposition,
        FlarkNativeSessionInspection,
        FlarkNativeSourceTransactionReceiptV1;
