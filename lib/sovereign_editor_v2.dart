/// Full Sovereign v2 API.
///
/// Most apps should import `package:sovereign_editor/sovereign_editor.dart`.
/// This barrel remains as an explicit full-v2 surface for integrations that
/// want the complete v2 type set from one import.
///
/// {@canonicalFor sovereign_command.SovereignCommandHandler}
/// {@canonicalFor sovereign_command.SovereignCommandPriority}
/// {@canonicalFor sovereign_command_registry.SovereignCommandRegistry}
/// {@canonicalFor sovereign_core_editing_commands.SovereignCoreEditingExtension}
/// {@canonicalFor sovereign_document.SovereignDocument}
/// {@canonicalFor sovereign_markdown_block_commands.SovereignMarkdownBlockEditingExtension}
/// {@canonicalFor sovereign_markdown_inline_commands.SovereignMarkdownInlineEditingExtension}
/// {@canonicalFor sovereign_markdown_inline_style.SovereignMarkdownInlineStyleMarker}
/// {@canonicalFor sovereign_markdown_input_commands.SovereignMarkdownInputEditingExtension}
/// {@canonicalFor sovereign_markdown_link_commands.SovereignApplyLinkEditPayload}
/// {@canonicalFor sovereign_markdown_link_commands.SovereignMarkdownLinkEditContext}
/// {@canonicalFor sovereign_markdown_link_commands.SovereignMarkdownLinkEditingExtension}
/// {@canonicalFor sovereign_markdown_parse_protocol.SovereignMarkdownParseProtocol}
/// {@canonicalFor sovereign_markdown_parse_result.SovereignMarkdownAmbiguityKind}
/// {@canonicalFor sovereign_markdown_parse_result.SovereignMarkdownAmbiguityZone}
/// {@canonicalFor sovereign_markdown_parse_result.SovereignMarkdownBlockKind}
/// {@canonicalFor sovereign_markdown_parse_result.SovereignMarkdownBlockNode}
/// {@canonicalFor sovereign_markdown_parse_result.SovereignMarkdownDiagnostic}
/// {@canonicalFor sovereign_markdown_parse_result.SovereignMarkdownHiddenRange}
/// {@canonicalFor sovereign_markdown_parse_result.SovereignMarkdownHiddenRangeKind}
/// {@canonicalFor sovereign_markdown_parse_result.SovereignMarkdownInlineKind}
/// {@canonicalFor sovereign_markdown_parse_result.SovereignMarkdownInlineToken}
/// {@canonicalFor sovereign_markdown_parse_result.SovereignMarkdownReplacementRange}
/// {@canonicalFor sovereign_markdown_parse_result.SovereignMarkdownReplacementRangeKind}
/// {@canonicalFor sovereign_markdown_profile.SovereignMarkdownProfileWire}
/// {@canonicalFor sovereign_markdown_table_commands.SovereignMarkdownTableEditingExtension}
/// {@canonicalFor sovereign_projected_text_edit_adapter.SovereignProjectedTextEditAdapter}
/// {@canonicalFor sovereign_projection.SovereignCursorMask}
/// {@canonicalFor sovereign_projection.SovereignHiddenRange}
/// {@canonicalFor sovereign_projection.SovereignHiddenRangeKind}
/// {@canonicalFor sovereign_projection.SovereignProjection}
/// {@canonicalFor sovereign_projection.SovereignProjectionAmbiguityKind}
/// {@canonicalFor sovereign_projection.SovereignProjectionAmbiguityZone}
/// {@canonicalFor sovereign_projection.SovereignProjectionPrediction}
/// {@canonicalFor sovereign_projection.SovereignProjectionReconciliation}
/// {@canonicalFor sovereign_projection.SovereignReplacementRange}
/// {@canonicalFor sovereign_projection.SovereignReplacementRangeKind}
/// {@canonicalFor sovereign_render_plan.SovereignRenderInlineActionKind}
/// {@canonicalFor sovereign_render_plan.SovereignRenderInlineRun}
/// {@canonicalFor sovereign_render_plan.SovereignRenderTableColumnAlignment}
/// {@canonicalFor sovereign_render_plan.SovereignRenderTextStyleToken}
/// {@canonicalFor sovereign_selection.SovereignMapAffinity}
/// {@canonicalFor sovereign_source_operation.SovereignSourceOperation}
/// {@canonicalFor sovereign_text_buffer.SovereignTextBuffer}
/// {@canonicalFor sovereign_utf8_utf16_mapper.SovereignUtf8Utf16Mapper}
library;

export 'src/v2/core/core.dart'
    show
        SovereignCommand,
        SovereignCommandContext,
        SovereignCommandHandler,
        SovereignCommandPriority,
        SovereignCommandRegistry,
        SovereignCommandResult,
        SovereignCommandResultKind,
        SovereignCoreEditingCommands,
        SovereignCoreEditingExtension,
        SovereignDocument,
        SovereignEditorRuntime,
        SovereignEditorRuntimeResult,
        SovereignEditorState,
        SovereignExtension,
        SovereignExtensionSet,
        SovereignInsertTextPayload,
        SovereignMapAffinity,
        SovereignSelection,
        SovereignSourceOperation,
        SovereignSourceRange,
        SovereignTextBuffer,
        SovereignTransaction,
        SovereignTransactionIntent,
        SovereignTransactionMetadata,
        SovereignUtf8Utf16Mapper;
export 'src/v2/flutter/flutter.dart'
    show
        SovereignCommandAction,
        SovereignCommandActions,
        SovereignCommandIntent,
        SovereignCommandInvocation,
        SovereignControllerEvent,
        SovereignControllerEventKind,
        SovereignFlutterController,
        SovereignCodeLanguageOption,
        SovereignMarkdownControllerCommands,
        MarkdownEditor,
        SovereignMarkdownEditingMode,
        SovereignMarkdownInteractionConfig,
        Markdown,
        SovereignLinkEditCallback,
        SovereignLinkOpenCallback,
        SovereignOverlayTargetWidgetBuilder,
        SovereignPreviewBlockWidgetBuilder,
        SovereignTypedCommandInvocation;
export 'src/v2/markdown/markdown.dart'
    show
        SovereignApplyLinkEditPayload,
        SovereignHandleBackspacePayload,
        SovereignHandleEnterPayload,
        SovereignInsertFencePayload,
        SovereignInsertLinkPayload,
        SovereignInsertTablePayload,
        SovereignInsertThematicBreakPayload,
        SovereignMarkdownAmbiguityKind,
        SovereignMarkdownAmbiguityZone,
        SovereignMarkdownBlockCommands,
        SovereignMarkdownBlockEditingExtension,
        SovereignMarkdownBlockKind,
        SovereignMarkdownBlockNode,
        SovereignMarkdownCommandCapabilities,
        SovereignMarkdownCommandQueries,
        SovereignMarkdownDiagnostic,
        SovereignMarkdownEditingExtensions,
        SovereignMarkdownHiddenRange,
        SovereignMarkdownHiddenRangeKind,
        SovereignMarkdownInlineCommands,
        SovereignMarkdownInlineEditingExtension,
        SovereignMarkdownInlineKind,
        SovereignMarkdownInlineStyle,
        SovereignMarkdownInlineStyleMarker,
        SovereignMarkdownInlineToken,
        SovereignMarkdownInputCommands,
        SovereignMarkdownInputEditingExtension,
        SovereignMarkdownLinkCommands,
        SovereignMarkdownLinkEditContext,
        SovereignMarkdownLinkEditingExtension,
        SovereignMarkdownParseBackend,
        SovereignMarkdownParseProtocol,
        SovereignMarkdownParseRequest,
        SovereignMarkdownParseResult,
        SovereignMarkdownParserCapabilities,
        SovereignMarkdownProfile,
        SovereignMarkdownProfileWire,
        SovereignMarkdownReplacementRange,
        SovereignMarkdownReplacementRangeKind,
        SovereignMarkdownTableCommands,
        SovereignMarkdownTableEditingExtension,
        SovereignNativeComrakParseBackend,
        SovereignSetHeadingLevelPayload,
        SovereignSetFenceLanguagePayload,
        SovereignSetTaskListCheckedPayload,
        SovereignRemoveLinkPayload,
        SovereignTableMutationPayload,
        SovereignToggleBulletListPayload,
        SovereignToggleInlineStylePayload,
        SovereignToggleOrderedListPayload,
        SovereignToggleQuotePayload,
        SovereignToggleTaskListPayload;
export 'src/v2/projection/projection.dart'
    show
        SovereignCursorMask,
        SovereignHiddenRange,
        SovereignHiddenRangeKind,
        SovereignProjectedTextEditAdapter,
        SovereignProjection,
        SovereignProjectionAmbiguityKind,
        SovereignProjectionAmbiguityZone,
        SovereignProjectionPrediction,
        SovereignProjectionReconciliation,
        SovereignReplacementRange,
        SovereignReplacementRangeKind;
export 'src/v2/render_plan/render_plan.dart'
    show
        SovereignRenderBlock,
        SovereignRenderCodeBlockDescriptor,
        SovereignRenderInlineActionDescriptor,
        SovereignRenderInlineActionKind,
        SovereignRenderInlineRun,
        SovereignRenderListItemDescriptor,
        SovereignRenderListKind,
        SovereignRenderOverlayKind,
        SovereignRenderOverlayPlan,
        SovereignRenderOverlayTarget,
        SovereignRenderPlan,
        SovereignRenderPlanContext,
        SovereignRenderPlanExtension,
        SovereignRenderTableColumnAlignment,
        SovereignRenderTableDescriptor,
        SovereignRenderTaskListItemDescriptor,
        SovereignRenderTextStyleToken,
        applySovereignRenderPlanExtensions;
export 'src/v2/native/native.dart'
    show
        NativeComrakBlockSpan,
        NativeComrakBridge,
        NativeComrakBridgeLoadException,
        NativeComrakBridgeLoadFailureKind,
        NativeComrakBridgePreflightResult,
        NativeComrakDiagnostic,
        NativeComrakInlineToken,
        NativeComrakParseInput,
        NativeComrakParseResult,
        NativeComrakPayloadCodec,
        NativeComrakProfile,
        NativeComrakRange,
        NativeComrakReplacementRange,
        createNativeComrakBridge,
        preflightNativeComrakBridge;
