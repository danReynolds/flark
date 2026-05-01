/// Public API for the Sovereign markdown editor and read-only preview widgets.
///
/// App code should import this library instead of deep implementation paths:
///
/// ```dart
/// import 'package:sovereign_editor/sovereign_editor.dart';
/// ```
///
/// The top-level library exposes editor/preview widgets, controller and command
/// APIs, theme models, syntax integration contracts, native parser diagnostics,
/// and model types that are currently part of those public signatures.
library;

export 'theme/sovereign_markdown_theme.dart';

// Primary editor API
export 'widgets/sovereign/controllers/sovereign_controller.dart';
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
export 'widgets/sovereign/engine/native_comrak_bridge_factory.dart';
export 'widgets/sovereign/engine/native_comrak_ffi.dart';
export 'widgets/sovereign/engine/syntax_engine.dart';
export 'widgets/sovereign/engine/syntax_snapshot.dart';
export 'widgets/sovereign/engine/syntax_types.dart';
export 'widgets/sovereign/engine/utf8_utf16_offset_mapper.dart';

// Public model types exposed by controller and syntax contracts.
export 'widgets/sovereign/models/block_node.dart';
export 'widgets/sovereign/models/block_tree.dart';
export 'widgets/sovereign/models/decoration_model.dart';
export 'widgets/sovereign/models/edit_op.dart';
export 'widgets/sovereign/models/geometry_model.dart';
export 'widgets/sovereign/models/line_index.dart';
export 'widgets/sovereign/models/sovereign_state.dart';
export 'widgets/sovereign/models/sovereign_style.dart';
