/// Session recording for dogfooding: the calls made to an editor, written
/// as a Dart repro and made again on a fresh editor.
library;

import 'dart:collection';
import 'dart:convert';

import '../../code.dart';
import '../parse/backend.dart';
import 'calls.dart';
import 'commands.dart';
import 'document.dart';
import 'editor.dart';
import 'projection.dart';

/// Records the calls made to one editor so that a session can be replayed as
/// a test. A host attaches one while dogfooding and copies [repro] when
/// something goes wrong.
///
/// The recorder observes the editor's calls ([FlarkEditor.onCall]) and keeps
/// where its window starts (source, selection, forced source mode) and each
/// call since, as the [FlarkEditorCall] it was, with what it returned. Its
/// two readers of a call are [repro], which writes it as Dart, and [replay],
/// which makes it again. A call keeps the change it made to the source
/// rather than a copy of the document, and past [capacity] calls the oldest
/// is folded into the window's start, so memory stays near one document
/// plus the recent changes. What the editor held before the window starts
/// beyond that is not replayed: its history (an Undo past the window's
/// start does nothing in the replay), a pending style and an unwritten table
/// cell the caret was in. The repro says when the window starts with any of
/// them.
final class FlarkEditRecorder {
  /// Records [editor]'s calls from now on, as its [FlarkEditor.onCall]. An
  /// editor serves one observer at a time: one that has another throws a
  /// [StateError].
  FlarkEditRecorder(this.editor, {this.capacity = 2000}) {
    if (capacity < 1) throw ArgumentError.value(capacity, 'capacity');
    if (editor.onCall != null) {
      throw StateError('the editor already has a call observer');
    }
    _restart();
    editor.onCall = _observer;
  }

  /// The editor recorded.
  final FlarkEditor editor;

  /// The most calls kept.
  final int capacity;

  late final _observer = _add;
  late String _source;
  late FlarkSelection _selection;
  late bool _sourceMode, _composing;

  /// The source the last call left, which the next one changes.
  late String _last;
  final _calls = ListQueue<_RecordedCall>();
  int _folded = 0;

  /// Whether the window starts with state a replay does not rebuild.
  bool _partial = false;

  /// The calls kept.
  int get length => _calls.length;

  /// Calls folded into the window's start because [capacity] was reached.
  int get folded => _folded;

  void _restart() {
    _source = _last = editor.source;
    _selection = editor.selection;
    _sourceMode = editor.sourceModeForced;
    _composing = editor.composing;
    _partial =
        editor.history.canUndo ||
        editor.hasPendingStyle ||
        editor.selection.tableCell != null;
    _calls.clear();
    _folded = 0;
  }

  /// Forget the calls so far: the window starts again at the editor's
  /// present state.
  void clear() => _restart();

  /// Stop recording. The calls kept stay.
  void detach() {
    if (editor.onCall == _observer) editor.onCall = null;
  }

  void _add(FlarkEditorCall call, bool? returned, Object? error) {
    final before = _last, after = editor.source;
    // A call refused for a stale revision is noted but not replayed: a
    // replay's revisions would not reproduce it.
    final stale =
        returned == false &&
        editor.lastRejection == FlarkRejection.staleRevision;
    final recorded = _RecordedCall(
      call,
      stale
          ? 'false, stale revision'
          : error != null
          ? 'threw $error'
          : returned == null
          ? 'done'
          : '$returned',
      replayed: !stale,
    );
    if (!identical(before, after) && before != after) {
      var start = 0;
      final shorter = before.length < after.length
          ? before.length
          : after.length;
      while (start < shorter &&
          before.codeUnitAt(start) == after.codeUnitAt(start)) {
        start++;
      }
      var end = 0;
      while (end < shorter - start &&
          before.codeUnitAt(before.length - 1 - end) ==
              after.codeUnitAt(after.length - 1 - end)) {
        end++;
      }
      recorded._change = (
        start,
        before.length - end - start,
        after.substring(start, after.length - end),
      );
    }
    _last = after;
    recorded
      .._selection = editor.selection
      .._sourceMode = editor.sourceModeForced
      .._composing = editor.composing;
    _calls.add(recorded);
    if (_calls.length > capacity) {
      final oldest = _calls.removeFirst();
      _source = oldest._applyTo(_source);
      _selection = oldest._selection;
      _sourceMode = oldest._sourceMode;
      _composing = oldest._composing;
      _partial = true;
      _folded++;
    }
  }

  /// A fresh editor over [backend] with the recorded editor's limits, after
  /// the recorded calls. [codeEditing] stands in for the recorded editor's
  /// delegate, which a replay of code edits needs to behave the same.
  FlarkEditor replay(
    FlarkParseBackend backend, {
    CodeEditingDelegate? codeEditing,
  }) {
    final replayed = FlarkEditor(
      backend,
      text: _source,
      caret: _selection.extent,
      codeEditing: codeEditing,
      syncLimit: editor.syncLimit,
      liveLimits: editor.liveLimits,
      sourceLimit: editor.sourceLimit,
      options: editor.options,
    );
    for (final call in _setup()) {
      _make(call, replayed);
    }
    for (final call in _calls) {
      if (call._replayed) _make(call._call, replayed);
    }
    return replayed;
  }

  /// The calls that put a fresh editor where the window starts. Source mode
  /// comes first: there any offset holds the caret, which the live document
  /// the editor opens with would move.
  List<FlarkEditorCall> _setup() => [
    if (_sourceMode) const SetSourceModeCall(true),
    if (_sourceMode || !_selection.isCollapsed)
      ApplyCall(SetSelection(_selection.base, _selection.extent), null),
    if (_composing) const CompositionCall(CompositionStep.begin),
  ];

  /// Dart that rebuilds the editor where the window starts, makes the
  /// recorded calls with the times history used, and expects the source and
  /// selection they ended with. As a regression test it passes until the
  /// behavior changes: correct the expectations to the intended result.
  String get repro {
    final out = StringBuffer()
      ..writeln(
        '// Flark repro: ${_calls.length} calls'
        '${_folded > 0 ? ', after $_folded earlier ones whose history is not replayed' : ''}.',
      );
    if (_partial) {
      out.writeln(
        '// The window starts mid-session: history, a pending style or an '
        'unwritten table cell from before it is not replayed.',
      );
    }
    if (editor.codeEditing != null) {
      out.writeln(
        '// The editor had a ${editor.codeEditing.runtimeType} code delegate; '
        'pass the same as codeEditing to replay code edits.',
      );
    }
    out
      ..writeln('final editor = FlarkEditor(')
      ..writeln('  createParseBackend(),')
      ..writeln('  text: ${_literal(_source)},')
      ..writeln('  caret: ${_selection.extent},');
    if (editor.syncLimit != FlarkEditor.defaultSyncLimit) {
      out.writeln('  syncLimit: ${editor.syncLimit},');
    }
    if (editor.sourceLimit != FlarkEditor.defaultSourceLimit) {
      out.writeln('  sourceLimit: ${editor.sourceLimit},');
    }
    final l = editor.liveLimits;
    const d = FlarkLiveLimits();
    if (l.lines != d.lines ||
        l.lineCodeUnits != d.lineCodeUnits ||
        l.blocks != d.blocks ||
        l.runs != d.runs ||
        l.blockCodeUnits != d.blockCodeUnits ||
        l.containerDepth != d.containerDepth) {
      out.writeln(
        '  liveLimits: const FlarkLiveLimits(lines: ${l.lines}, '
        'lineCodeUnits: ${l.lineCodeUnits}, blocks: ${l.blocks}, '
        'runs: ${l.runs}, blockCodeUnits: ${l.blockCodeUnits}, '
        'containerDepth: ${l.containerDepth}),',
      );
    }
    final options = editor.options;
    if (!options.softBreakAsNewline || !options.editableDelimiterRows) {
      out.writeln(
        '  options: const ProjectionOptions('
        '${options.softBreakAsNewline ? '' : 'softBreakAsNewline: false, '}'
        '${options.editableDelimiterRows ? '' : 'editableDelimiterRows: false'}'
        '),',
      );
    }
    out.writeln(');');
    for (final call in _setup()) {
      out.writeln('${_dart(call)};');
    }
    var source = _source;
    var selection = _selection;
    for (final call in _calls) {
      // A result is a line comment: an error's message may hold line breaks.
      final result = call._result
          .replaceAll('\r', r'\r')
          .replaceAll('\n', r'\n');
      final code = _dart(call._call);
      out.writeln(
        call._replayed ? '$code; // $result' : '// $code; // $result',
      );
      source = call._applyTo(source);
      selection = call._selection;
    }
    out
      ..writeln('expect(editor.source, ${_literal(source)});')
      ..writeln(
        'expect((editor.selection.base, editor.selection.extent), '
        '(${selection.base}, ${selection.extent}));',
      );
    return out.toString();
  }

  /// [call] as the Dart that makes it on `editor`.
  static String _dart(FlarkEditorCall call) => switch (call) {
    ApplyCall(:final command, :final at, :final afterComposition) =>
      'editor.${afterComposition ? 'applyAfterComposition' : 'apply'}'
          '(${describeCommand(command)}'
          '${at == null ? '' : ', at: const Duration(microseconds: ${at.inMicroseconds})'})',
    ReplaceSourceRangeCall(
      :final start,
      :final end,
      :final text,
      :final replaceAll,
    ) =>
      'editor.replaceSourceRange($start, $end, ${_literal(text)}'
          '${replaceAll ? ', replaceAll: true' : ''})',
    LoadMarkdownCall(:final text) => 'editor.loadMarkdown(${_literal(text)})',
    SelectAllCall(:final codeBlock) =>
      'editor.selectAll(${codeBlock ? 'codeBlock: true' : ''})',
    CompositionCall(:final step) => 'editor.${step.name}Composition()',
    SetSourceModeCall(:final enabled) => 'editor.setSourceMode($enabled)',
  };

  /// Makes [call] on [editor] again.
  static void _make(FlarkEditorCall call, FlarkEditor editor) {
    switch (call) {
      case ApplyCall(:final command, :final at, afterComposition: true):
        editor.applyAfterComposition(command, at: at);
      case ApplyCall(:final command, :final at):
        editor.apply(command, at: at);
      case ReplaceSourceRangeCall(
        :final start,
        :final end,
        :final text,
        :final replaceAll,
      ):
        editor.replaceSourceRange(start, end, text, replaceAll: replaceAll);
      case LoadMarkdownCall(:final text):
        editor.loadMarkdown(text);
      case SelectAllCall(:final codeBlock):
        editor.selectAll(codeBlock: codeBlock);
      case CompositionCall(step: CompositionStep.begin):
        editor.beginComposition();
      case CompositionCall(step: CompositionStep.commit):
        editor.commitComposition();
      case CompositionCall(step: CompositionStep.cancel):
        editor.cancelComposition();
      case SetSourceModeCall(:final enabled):
        editor.setSourceMode(enabled);
    }
  }

  /// [command] as the Dart that constructs it.
  static String describeCommand(FlarkCommand command) =>
      'const ${switch (command) {
        InsertText(:final text) => 'InsertText(${_literal(text)})',
        Paste(:final text) => 'Paste(${_literal(text)})',
        DeleteBackward(:final word) => 'DeleteBackward(${word ? 'word: true' : ''})',
        DeleteForward(:final word) => 'DeleteForward(${word ? 'word: true' : ''})',
        Newline(:final paragraph) => 'Newline(${paragraph ? 'paragraph: true' : ''})',
        ReplaceRange(:final start, :final end, :final text) => 'ReplaceRange($start, $end, ${_literal(text)})',
        SetSelection(:final base, :final extent) => 'SetSelection($base, $extent)',
        SelectAll() => 'SelectAll()',
        PlaceCaret(:final row, :final offset, :final leadingHalf, :final extend) => 'PlaceCaret($row, $offset, leadingHalf: $leadingHalf, extend: $extend)',
        MoveCaret(:final direction, :final unit, :final extend) => 'MoveCaret(MoveDirection.${direction.name}, unit: MoveUnit.${unit.name}, extend: $extend)',
        MoveTableCell(:final backward) => 'MoveTableCell(${backward ? 'backward: true' : ''})',
        Undo() => 'Undo()',
        Redo() => 'Redo()',
        ToggleTask() => 'ToggleTask()',
        Indent() => 'Indent()',
        Outdent() => 'Outdent()',
        ToggleStyle(:final style) => 'ToggleStyle(${_style(style)})',
        SetStyle(:final style, :final enabled) => 'SetStyle(${_style(style)}, enabled: $enabled)',
        SetHeadingLevel(:final level) => 'SetHeadingLevel($level)',
        SetCodeLanguage(:final language) => 'SetCodeLanguage(${_literal(language)})',
        SetLink(:final destination, :final text, :final title) => 'SetLink(${_literal(destination)}${text == null ? '' : ', text: ${_literal(text)}'}${title == null ? '' : ', title: ${_literal(title)}'})',
        SetImage(:final destination, :final alt, :final title) => 'SetImage(${_literal(destination)}${alt == null ? '' : ', alt: ${_literal(alt)}'}${title == null ? '' : ', title: ${_literal(title)}'})',
        RemoveLink() => 'RemoveLink()',
        RemoveImage() => 'RemoveImage()',
      }}';

  static String _style(int style) => switch (style) {
    Style.emphasis => 'Style.emphasis',
    Style.strong => 'Style.strong',
    Style.code => 'Style.code',
    Style.strikethrough => 'Style.strikethrough',
    _ => '$style',
  };

  /// [text] as a Dart string literal: JSON escapes it, and a dollar sign,
  /// which JSON leaves alone, would start an interpolation.
  static String _literal(String text) =>
      jsonEncode(text).replaceAll(r'$', r'\$');
}

/// One recorded call: the call, what it returned, whether a replay makes it
/// again (not one refused for a stale revision, which a replay's revisions
/// would not reproduce), and the change it made with the state it left.
final class _RecordedCall {
  _RecordedCall(this._call, this._result, {required bool replayed})
    : _replayed = replayed;

  final FlarkEditorCall _call;
  final String _result;
  final bool _replayed;
  (int, int, String)? _change;
  late FlarkSelection _selection;
  late bool _sourceMode, _composing;

  String _applyTo(String source) {
    final change = _change;
    if (change == null) return source;
    final (start, removed, inserted) = change;
    return source.replaceRange(start, start + removed, inserted);
  }
}
