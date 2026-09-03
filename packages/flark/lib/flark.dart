/// Flark v5 kernel. M1 exposes the render model and the parse transports.
///
/// On the Dart VM, [createParseBackend] returns the FFI transport. On the web
/// it throws; create a `WasmParseBackend` with `load()` or `fromBytes()`
/// instead. Both classes are exported for the platform they exist on.
library;

export 'src/parse/backend.dart' show FlarkParseBackend, FlarkParseException;
export 'src/parse/parse.dart';
export 'src/parse/render_model.dart';
export 'src/parse/schema.g.dart';
export 'src/kernel/commands.dart';
export 'src/kernel/document.dart' show FlarkDocument, FlarkSelection, Owner;
export 'src/kernel/editor.dart' show FlarkEditor, FlarkListener;
export 'src/kernel/history.dart' show History, HistoryEntry, PendingStyle;
export 'src/kernel/projection.dart';
