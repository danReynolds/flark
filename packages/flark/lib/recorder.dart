/// A debugging aid for dogfooding: [FlarkEditRecorder] records the calls
/// made to a [FlarkEditor] (attach one with `editor.recorder = ...`) and
/// writes them as a Dart repro that replays the session as a test. It is kept
/// out of `package:flark/flark.dart` because production code never needs it.
library;

export 'src/kernel/editor.dart' show FlarkEditRecorder;
