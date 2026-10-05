/// A debugging aid for dogfooding: a [FlarkEditRecorder] records the calls
/// made to a [FlarkEditor] (`FlarkEditRecorder(editor)`), each a
/// [FlarkEditorCall], and writes them as a Dart repro that replays the
/// session as a test. It is kept out of `package:flark/flark.dart` because
/// production code never needs it.
library;

export 'src/kernel/calls.dart';
export 'src/kernel/recorder.dart' show FlarkEditRecorder;
