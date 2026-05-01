export 'theme/dune_markdown_theme.dart';

// Primary editor API
export 'widgets/sovereign/controllers/sovereign_controller.dart';
export 'widgets/sovereign/core/pipeline/edit_differ.dart';
export 'widgets/sovereign/controllers/undo_stack.dart';
export 'widgets/sovereign/commands/sovereign_markdown_commands.dart';
export 'widgets/sovereign/commands/models/sovereign_block_style.dart';
export 'widgets/sovereign/commands/models/sovereign_command_capabilities.dart';
export 'widgets/sovereign/commands/models/sovereign_command_result.dart';
export 'widgets/sovereign/commands/models/sovereign_inline_style.dart';
export 'widgets/sovereign/commands/models/sovereign_link_edit_context.dart';
export 'widgets/sovereign/presentation/sovereign_editor.dart';
export 'widgets/sovereign/presentation/sovereign_markdown_view.dart';
export 'widgets/sovereign/theme/sovereign_editor_theme.dart';

// Engine and syntax pipeline (advanced, but supported)
export 'widgets/sovereign/engine/commonmark_parse_backend.dart';
export 'widgets/sovereign/engine/commonmark_syntax_engine_adapter.dart';
export 'widgets/sovereign/engine/native_comrak_bridge_factory.dart';
export 'widgets/sovereign/engine/native_comrak_ffi.dart';
export 'widgets/sovereign/engine/native_comrak_parse_backend.dart';
export 'widgets/sovereign/engine/syntax_engine.dart';
export 'widgets/sovereign/engine/syntax_engine_factory.dart';
export 'widgets/sovereign/engine/syntax_parse_scheduler.dart';
export 'widgets/sovereign/engine/syntax_snapshot.dart';
export 'widgets/sovereign/engine/syntax_types.dart';
export 'widgets/sovereign/engine/utf8_utf16_offset_mapper.dart';
export 'widgets/sovereign/engine/v1_syntax_engine_adapter.dart';

// Low-level logic/models (useful for tests and tooling; may evolve faster)
export 'widgets/sovereign/logic/block_parser.dart';
export 'widgets/sovereign/logic/fenced_code_scanner.dart';
export 'widgets/sovereign/logic/markdown_marker_grammar.dart';
export 'widgets/sovereign/logic/projector.dart';
export 'widgets/sovereign/logic/sovereign_code_highlighter.dart';
export 'widgets/sovereign/logic/sovereign_geometry_scanner.dart';
export 'widgets/sovereign/logic/sovereign_markdown_markers.dart';
export 'widgets/sovereign/logic/sovereign_style_scanner.dart';
export 'widgets/sovereign/models/block_node.dart';
export 'widgets/sovereign/models/block_tree.dart';
export 'widgets/sovereign/models/decoration_model.dart';
export 'widgets/sovereign/models/edit_op.dart';
export 'widgets/sovereign/models/geometry_model.dart';
export 'widgets/sovereign/models/line_index.dart';
export 'widgets/sovereign/models/sovereign_state.dart';
export 'widgets/sovereign/models/sovereign_style.dart';
