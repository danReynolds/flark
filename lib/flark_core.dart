/// Headless Flark v2 markdown editing core.
///
/// This barrel contains no Flutter widgets. It is intended for tests, command
/// integrations, server-side render-plan generation, and consumers that want
/// direct access to the source-first document/runtime/projection model.
library;

export 'src/v2/core/core.dart';
export 'src/v2/markdown/markdown.dart' hide FlarkNativeComrakParseBackend;
export 'src/v2/projection/projection.dart';
export 'src/v2/render_plan/render_plan.dart';
