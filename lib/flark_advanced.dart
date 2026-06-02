/// Full Flark v2 API.
///
/// Most apps should import `package:flark/flark.dart`.
/// This barrel remains as an explicit full-v2 surface for integrations that
/// want the complete v2 type set from one import.
///
/// {@canonicalFor sovereign_command.FlarkCommandHandler}
/// {@canonicalFor sovereign_command.FlarkCommandPriority}
/// {@canonicalFor sovereign_command_registry.FlarkCommandRegistry}
/// {@canonicalFor sovereign_core_editing_commands.FlarkCoreEditingExtension}
/// {@canonicalFor sovereign_document.FlarkDocument}
/// {@canonicalFor sovereign_markdown_block_commands.FlarkMarkdownBlockEditingExtension}
/// {@canonicalFor sovereign_markdown_inline_commands.FlarkMarkdownInlineEditingExtension}
/// {@canonicalFor sovereign_markdown_inline_style.FlarkMarkdownInlineStyleMarker}
/// {@canonicalFor sovereign_markdown_input_commands.FlarkMarkdownInputEditingExtension}
/// {@canonicalFor sovereign_markdown_link_commands.FlarkApplyLinkEditPayload}
/// {@canonicalFor sovereign_markdown_link_commands.FlarkMarkdownLinkEditContext}
/// {@canonicalFor sovereign_markdown_link_commands.FlarkMarkdownLinkEditingExtension}
/// {@canonicalFor sovereign_markdown_parse_protocol.FlarkMarkdownParseProtocol}
/// {@canonicalFor sovereign_markdown_parse_result.FlarkMarkdownAmbiguityKind}
/// {@canonicalFor sovereign_markdown_parse_result.FlarkMarkdownAmbiguityZone}
/// {@canonicalFor sovereign_markdown_parse_result.FlarkMarkdownBlockKind}
/// {@canonicalFor sovereign_markdown_parse_result.FlarkMarkdownBlockNode}
/// {@canonicalFor sovereign_markdown_parse_result.FlarkMarkdownDiagnostic}
/// {@canonicalFor sovereign_markdown_parse_result.FlarkMarkdownHiddenRange}
/// {@canonicalFor sovereign_markdown_parse_result.FlarkMarkdownHiddenRangeKind}
/// {@canonicalFor sovereign_markdown_parse_result.FlarkMarkdownInlineKind}
/// {@canonicalFor sovereign_markdown_parse_result.FlarkMarkdownInlineToken}
/// {@canonicalFor sovereign_markdown_parse_result.FlarkMarkdownReplacementRange}
/// {@canonicalFor sovereign_markdown_parse_result.FlarkMarkdownReplacementRangeKind}
/// {@canonicalFor sovereign_markdown_profile.FlarkMarkdownProfileWire}
/// {@canonicalFor sovereign_markdown_table_commands.FlarkMarkdownTableEditingExtension}
/// {@canonicalFor sovereign_projected_text_edit_adapter.FlarkProjectedTextEditAdapter}
/// {@canonicalFor sovereign_projection.FlarkCursorMask}
/// {@canonicalFor sovereign_projection.FlarkHiddenRange}
/// {@canonicalFor sovereign_projection.FlarkHiddenRangeKind}
/// {@canonicalFor sovereign_projection.FlarkProjection}
/// {@canonicalFor sovereign_projection.FlarkProjectionAmbiguityKind}
/// {@canonicalFor sovereign_projection.FlarkProjectionAmbiguityZone}
/// {@canonicalFor sovereign_projection.FlarkProjectionPrediction}
/// {@canonicalFor sovereign_projection.FlarkProjectionReconciliation}
/// {@canonicalFor sovereign_projection.FlarkReplacementRange}
/// {@canonicalFor sovereign_projection.FlarkReplacementRangeKind}
/// {@canonicalFor sovereign_render_plan.FlarkRenderInlineActionKind}
/// {@canonicalFor sovereign_render_plan.FlarkRenderInlineRun}
/// {@canonicalFor sovereign_render_plan.FlarkRenderTableColumnAlignment}
/// {@canonicalFor sovereign_render_plan.FlarkRenderTextStyleToken}
/// {@canonicalFor sovereign_selection.FlarkMapAffinity}
/// {@canonicalFor sovereign_source_operation.FlarkSourceOperation}
/// {@canonicalFor sovereign_text_buffer.FlarkTextBuffer}
/// {@canonicalFor sovereign_utf8_utf16_mapper.FlarkUtf8Utf16Mapper}
library;

export 'src/v2/core/core.dart'
    show
        FlarkCommand,
        FlarkCommandContext,
        FlarkCommandHandler,
        FlarkCommandPriority,
        FlarkCommandRegistry,
        FlarkCommandResult,
        FlarkCommandResultKind,
        FlarkCoreEditingCommands,
        FlarkCoreEditingExtension,
        FlarkDocument,
        FlarkEditorRuntime,
        FlarkEditorRuntimeResult,
        FlarkEditorState,
        FlarkExtension,
        FlarkExtensionSet,
        FlarkInsertTextPayload,
        FlarkMapAffinity,
        FlarkSelection,
        FlarkSourceOperation,
        FlarkSourceRange,
        FlarkTextBuffer,
        FlarkTransaction,
        FlarkTransactionIntent,
        FlarkTransactionMetadata,
        FlarkUtf8Utf16Mapper;
export 'src/v2/flutter/flutter.dart'
    show
        FlarkCommandAction,
        FlarkCommandActions,
        FlarkCommandIntent,
        FlarkCommandInvocation,
        FlarkControllerEvent,
        FlarkControllerEventKind,
        FlarkFlutterController,
        FlarkCodeLanguageOption,
        FlarkMarkdownControllerCommands,
        MarkdownEditor,
        FlarkMarkdownEditingMode,
        FlarkMarkdownInteractionConfig,
        Markdown,
        FlarkLinkEditCallback,
        FlarkLinkOpenCallback,
        FlarkOverlayTargetWidgetBuilder,
        FlarkPreviewBlockWidgetBuilder,
        FlarkTypedCommandInvocation;
export 'src/v2/markdown/markdown.dart'
    show
        FlarkApplyLinkEditPayload,
        FlarkHandleBackspacePayload,
        FlarkHandleEnterPayload,
        FlarkInsertFencePayload,
        FlarkInsertLinkPayload,
        FlarkInsertTablePayload,
        FlarkInsertThematicBreakPayload,
        FlarkMarkdownAmbiguityKind,
        FlarkMarkdownAmbiguityZone,
        FlarkMarkdownBlockCommands,
        FlarkMarkdownBlockEditingExtension,
        FlarkMarkdownBlockKind,
        FlarkMarkdownBlockNode,
        FlarkMarkdownCommandCapabilities,
        FlarkMarkdownCommandQueries,
        FlarkMarkdownDiagnostic,
        FlarkMarkdownEditingExtensions,
        FlarkMarkdownHiddenRange,
        FlarkMarkdownHiddenRangeKind,
        FlarkMarkdownInlineCommands,
        FlarkMarkdownInlineEditingExtension,
        FlarkMarkdownInlineKind,
        FlarkMarkdownInlineStyle,
        FlarkMarkdownInlineStyleMarker,
        FlarkMarkdownInlineToken,
        FlarkMarkdownInputCommands,
        FlarkMarkdownInputEditingExtension,
        FlarkMarkdownLinkCommands,
        FlarkMarkdownLinkEditContext,
        FlarkMarkdownLinkEditingExtension,
        FlarkMarkdownParseBackend,
        FlarkMarkdownParseProtocol,
        FlarkMarkdownParseRequest,
        FlarkMarkdownParseResult,
        FlarkMarkdownParserCapabilities,
        FlarkMarkdownProfile,
        FlarkMarkdownProfileWire,
        FlarkMarkdownReplacementRange,
        FlarkMarkdownReplacementRangeKind,
        FlarkMarkdownTableCommands,
        FlarkMarkdownTableEditingExtension,
        FlarkNativeComrakParseBackend,
        FlarkSetHeadingLevelPayload,
        FlarkSetFenceLanguagePayload,
        FlarkSetTaskListCheckedPayload,
        FlarkRemoveLinkPayload,
        FlarkTableMutationPayload,
        FlarkToggleBulletListPayload,
        FlarkToggleInlineStylePayload,
        FlarkToggleOrderedListPayload,
        FlarkToggleQuotePayload,
        FlarkToggleTaskListPayload;
export 'src/v2/projection/projection.dart'
    show
        FlarkCursorMask,
        FlarkHiddenRange,
        FlarkHiddenRangeKind,
        FlarkProjectedTextEditAdapter,
        FlarkProjection,
        FlarkProjectionAmbiguityKind,
        FlarkProjectionAmbiguityZone,
        FlarkProjectionPrediction,
        FlarkProjectionReconciliation,
        FlarkReplacementRange,
        FlarkReplacementRangeKind;
export 'src/v2/render_plan/render_plan.dart'
    show
        FlarkRenderBlock,
        FlarkRenderCodeBlockDescriptor,
        FlarkRenderInlineActionDescriptor,
        FlarkRenderInlineActionKind,
        FlarkRenderInlineRun,
        FlarkRenderListItemDescriptor,
        FlarkRenderListKind,
        FlarkRenderOverlayKind,
        FlarkRenderOverlayPlan,
        FlarkRenderOverlayTarget,
        FlarkRenderPlan,
        FlarkRenderPlanContext,
        FlarkRenderPlanExtension,
        FlarkRenderTableColumnAlignment,
        FlarkRenderTableDescriptor,
        FlarkRenderTaskListItemDescriptor,
        FlarkRenderTextStyleToken,
        applyFlarkRenderPlanExtensions;
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
