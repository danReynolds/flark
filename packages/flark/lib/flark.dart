/// The Flark editing kernel without a UI: the parse transports, the
/// [FlarkEditor] facade with its closed command set and grouped history, and
/// the projection that hosts paint. Most apps use a host package, or
/// `package:flark/session.dart`, which loads and owns the parser itself.
///
/// On the Dart VM, [createParseBackend] returns a native parser. On the web it
/// throws; load the bundled module with `WasmParseBackend.bundled()` from
/// `package:flark/wasm.dart` instead. Whoever creates a parser owns it and
/// calls [FlarkParseBackend.dispose] when done.
library;

// The render model and its schema constants are the parse crate's contract,
// exported from package:flark/render_model.dart for hosts that read it.

export 'src/parse/backend.dart' show FlarkParseBackend, FlarkParseException;
export 'src/parse/parse.dart';
export 'src/kernel/commands.dart';
export 'src/kernel/style_state.dart';
export 'src/kernel/document.dart' show FlarkDocument, FlarkSelection, Owner;
export 'src/kernel/editor.dart'
    show
        FlarkEditor,
        FlarkEditorSnapshot,
        FlarkListener,
        FlarkLiveSnapshot,
        FlarkSourceSnapshot,
        FlarkLiveLimits,
        FlarkRejection;
export 'src/kernel/history.dart' show History, HistoryEntry, PendingStyle;
export 'src/kernel/projection.dart';
export 'src/kernel/resource.dart';
