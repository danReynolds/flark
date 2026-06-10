/// Full Flark v2 API.
///
/// Most apps should import `package:flark/flark.dart`.
/// This barrel remains as an explicit full-v2 surface for integrations that
/// want the complete v2 type set from one import.
///
/// {@canonicalFor flark_command.FlarkCommandHandler}
/// {@canonicalFor flark_command.FlarkCommandPriority}
/// {@canonicalFor flark_command_registry.FlarkCommandRegistry}
/// {@canonicalFor flark_core_editing_commands.FlarkCoreEditingExtension}
/// {@canonicalFor flark_document.FlarkDocument}
/// {@canonicalFor flark_markdown_block_commands.FlarkMarkdownBlockEditingExtension}
/// {@canonicalFor flark_markdown_command_capabilities.FlarkMarkdownCommandCapabilities}
/// {@canonicalFor flark_markdown_command_capabilities.FlarkMarkdownCommandQueries}
/// {@canonicalFor flark_markdown_inline_commands.FlarkMarkdownInlineEditingExtension}
/// {@canonicalFor flark_markdown_inline_style.FlarkMarkdownInlineStyleMarker}
/// {@canonicalFor flark_markdown_input_commands.FlarkMarkdownInputEditingExtension}
/// {@canonicalFor flark_markdown_link_commands.FlarkApplyLinkEditPayload}
/// {@canonicalFor flark_markdown_link_commands.FlarkMarkdownLinkEditContext}
/// {@canonicalFor flark_markdown_link_commands.FlarkMarkdownLinkEditingExtension}
/// {@canonicalFor flark_markdown_parse_protocol.FlarkMarkdownParseProtocol}
/// {@canonicalFor flark_markdown_parse_result.FlarkMarkdownAmbiguityKind}
/// {@canonicalFor flark_markdown_parse_result.FlarkMarkdownAmbiguityZone}
/// {@canonicalFor flark_markdown_parse_result.FlarkMarkdownBlockKind}
/// {@canonicalFor flark_markdown_parse_result.FlarkMarkdownBlockNode}
/// {@canonicalFor flark_markdown_parse_result.FlarkMarkdownDiagnostic}
/// {@canonicalFor flark_markdown_parse_result.FlarkMarkdownHiddenRange}
/// {@canonicalFor flark_markdown_parse_result.FlarkMarkdownHiddenRangeKind}
/// {@canonicalFor flark_markdown_parse_result.FlarkMarkdownInlineKind}
/// {@canonicalFor flark_markdown_parse_result.FlarkMarkdownInlineToken}
/// {@canonicalFor flark_markdown_parse_result.FlarkMarkdownReplacementRange}
/// {@canonicalFor flark_markdown_parse_result.FlarkMarkdownReplacementRangeKind}
/// {@canonicalFor flark_markdown_profile.FlarkMarkdownProfileWire}
/// {@canonicalFor flark_markdown_table_commands.FlarkMarkdownTableEditingExtension}
/// {@canonicalFor flark_native_comrak_parse_backend.FlarkNativeComrakParseProfile}
/// {@canonicalFor flark_native_comrak_parse_backend.FlarkNativeComrakProfiledParseResult}
/// {@canonicalFor flark_projected_text_edit_adapter.FlarkProjectedTextEditAdapter}
/// {@canonicalFor flark_projection.FlarkCursorMask}
/// {@canonicalFor flark_projection.FlarkHiddenRange}
/// {@canonicalFor flark_projection.FlarkHiddenRangeKind}
/// {@canonicalFor flark_projection.FlarkProjection}
/// {@canonicalFor flark_projection.FlarkProjectionAmbiguityKind}
/// {@canonicalFor flark_projection.FlarkProjectionAmbiguityZone}
/// {@canonicalFor flark_projection.FlarkProjectionPrediction}
/// {@canonicalFor flark_projection.FlarkProjectionReconciliation}
/// {@canonicalFor flark_projection.FlarkReplacementRange}
/// {@canonicalFor flark_projection.FlarkReplacementRangeKind}
/// {@canonicalFor flark_render_plan.FlarkRenderInlineActionKind}
/// {@canonicalFor flark_render_plan.FlarkRenderInlineRun}
/// {@canonicalFor flark_render_plan.FlarkRenderTableColumnAlignment}
/// {@canonicalFor flark_render_plan.FlarkRenderTextStyleToken}
/// {@canonicalFor flark_selection.FlarkMapAffinity}
/// {@canonicalFor flark_source_operation.FlarkSourceOperation}
/// {@canonicalFor flark_text_buffer.FlarkTextBuffer}
/// {@canonicalFor flark_utf8_utf16_mapper.FlarkUtf8Utf16Mapper}
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
        FlarkMarkdownCommands,
        FlarkMarkdownControllerCommandFacade,
        FlarkMarkdownControllerCommands,
        MarkdownEditorFormField,
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
        FlarkNativeComrakParseProfile,
        FlarkNativeComrakParseBackend,
        FlarkNativeComrakProfiledParseResult,
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
        FlarkRenderPlanFidelity,
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
        flarkNativeParseIsolateThresholdBytes,
        preflightNativeComrakBridge;
