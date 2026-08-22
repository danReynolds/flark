/// Headless Flark v2 markdown editing core.
///
/// This barrel contains no Flutter widgets. It is intended for tests, command
/// integrations, server-side render-plan generation, and consumers that want
/// direct access to the source-first document/runtime/projection model.
///
/// {@canonicalFor flark_history_stack.FlarkHistoryEntry}
/// {@canonicalFor flark_history_stack.FlarkHistoryResult}
/// {@canonicalFor flark_history_stack.FlarkHistoryStack}
/// {@canonicalFor flark_inline_segmentation.FlarkInlineSegment}
/// {@canonicalFor flark_inline_segmentation.flarkSegmentInlineRuns}
/// {@canonicalFor flark_markdown_command_capabilities.FlarkInlineRunRange}
/// {@canonicalFor flark_markdown_image_commands.FlarkApplyImageEditPayload}
/// {@canonicalFor flark_markdown_image_commands.FlarkMarkdownImageCommands}
/// {@canonicalFor flark_markdown_image_commands.FlarkMarkdownImageEditContext}
/// {@canonicalFor flark_markdown_image_commands.FlarkMarkdownImageEditingExtension}
/// {@canonicalFor flark_markdown_image_commands.FlarkRemoveImagePayload}
/// {@canonicalFor flark_projected_text_edit_adapter.FlarkProjectedEditResolution}
/// {@canonicalFor flark_render_plan.FlarkRenderTableCellDescriptor}
/// {@canonicalFor flark_render_plan.FlarkRenderTableRowDescriptor}
/// {@canonicalFor flark_render_reconciler.FlarkRenderAdoption}
/// {@canonicalFor flark_render_reconciler.FlarkRenderReconciler}
library;

export 'src/v2/core/core.dart';
export 'src/v2/markdown/markdown.dart' hide FlarkNativeComrakParseBackend;
export 'src/v2/projection/projection.dart';
export 'src/v2/render_plan/render_plan.dart';
