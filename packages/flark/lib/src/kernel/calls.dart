/// The calls a host makes to a [FlarkEditor] that can change it, as data:
/// what [FlarkEditor.onCall] is told of, and what a recorder writes out as
/// Dart or makes again on another editor.
library;

import 'commands.dart';
import 'editor.dart';

/// One call to [FlarkEditor]'s editing API.
sealed class FlarkEditorCall {
  const FlarkEditorCall();
}

/// [FlarkEditor.apply], or [FlarkEditor.applyAfterComposition] when
/// [afterComposition], of [command] at [at]: the time history used, or null
/// for none given, when the editor's clock gives it.
final class ApplyCall extends FlarkEditorCall {
  const ApplyCall(this.command, this.at, {this.afterComposition = false});

  final FlarkCommand command;
  final Duration? at;
  final bool afterComposition;
}

/// [FlarkEditor.replaceSourceRange].
final class ReplaceSourceRangeCall extends FlarkEditorCall {
  const ReplaceSourceRangeCall(
    this.start,
    this.end,
    this.text, {
    this.replaceAll = false,
  });

  final int start, end;
  final String text;
  final bool replaceAll;
}

/// [FlarkEditor.loadMarkdown].
final class LoadMarkdownCall extends FlarkEditorCall {
  const LoadMarkdownCall(this.text);

  final String text;
}

/// [FlarkEditor.selectAll].
final class SelectAllCall extends FlarkEditorCall {
  const SelectAllCall({this.codeBlock = false});

  final bool codeBlock;
}

/// The three steps of a composition: [FlarkEditor.beginComposition],
/// [FlarkEditor.commitComposition] and [FlarkEditor.cancelComposition].
enum CompositionStep { begin, commit, cancel }

/// A [CompositionStep] that did something: one that began a composition
/// while none was open, or ended the open one.
final class CompositionCall extends FlarkEditorCall {
  const CompositionCall(this.step);

  final CompositionStep step;
}

/// [FlarkEditor.setSourceMode].
final class SetSourceModeCall extends FlarkEditorCall {
  const SetSourceModeCall(this.enabled);

  final bool enabled;
}
