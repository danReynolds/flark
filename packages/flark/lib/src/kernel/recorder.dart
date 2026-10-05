part of 'editor.dart';

/// Records the calls made to one editor so that a session can be replayed as
/// a test. A host attaches one while dogfooding ([FlarkEditor.recorder]) and
/// copies [repro] when something goes wrong.
///
/// The recorder keeps where its window starts (source, selection, forced
/// source mode) and each call since, with the time history used and what it
/// returned. A call keeps the change it made to the source rather than a copy
/// of the document, and past [capacity] calls the oldest is folded into the
/// window's start, so memory stays near one document plus the recent changes.
/// What the editor held before the window starts beyond that is not
/// replayed: its history (an Undo past the window's start does nothing in
/// the replay), a pending style and an unwritten table cell the caret was
/// in. The repro says when the window starts with any of them.
final class FlarkEditRecorder {
  FlarkEditRecorder({this.capacity = 2000}) {
    if (capacity < 1) throw ArgumentError.value(capacity, 'capacity');
  }

  /// The most calls kept.
  final int capacity;

  FlarkEditor? _editor;
  late String _source;
  late FlarkSelection _selection;
  late bool _sourceMode, _composing;
  final _calls = ListQueue<_RecordedCall>();
  int _folded = 0;

  /// Whether the window starts with state a replay does not rebuild.
  bool _partial = false;

  /// The calls kept.
  int get length => _calls.length;

  /// Calls folded into the window's start because [capacity] was reached.
  int get folded => _folded;

  void _attach(FlarkEditor editor) {
    final current = _editor;
    if (current != null && !identical(current, editor)) {
      throw StateError('a FlarkEditRecorder records one editor');
    }
    _editor = editor;
    _restart(editor);
  }

  void _restart(FlarkEditor editor) {
    _source = editor.source;
    _selection = editor.selection;
    _sourceMode = editor._forceSourceMode;
    _composing = editor.composing;
    _partial =
        editor.history.canUndo ||
        editor._pending != null ||
        editor.selection.tableCell != null;
    _calls.clear();
    _folded = 0;
  }

  /// Forget the calls so far: the window starts again at the editor's
  /// present state.
  void clear() {
    final editor = _editor;
    if (editor != null) _restart(editor);
  }

  void _add(FlarkEditor editor, String before, _RecordedCall call) {
    final after = editor.source;
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
      call._change = (
        start,
        before.length - end - start,
        after.substring(start, after.length - end),
      );
    }
    call
      .._selection = editor.selection
      .._sourceMode = editor._forceSourceMode
      .._composing = editor.composing;
    _calls.add(call);
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
    final recorded = _editor;
    if (recorded == null) throw StateError('nothing was recorded');
    final editor = FlarkEditor(
      backend,
      text: _source,
      caret: _selection.extent,
      codeEditing: codeEditing,
      syncLimit: recorded.syncLimit,
      liveLimits: recorded.liveLimits,
      sourceLimit: recorded.sourceLimit,
      options: recorded._options,
    );
    for (final step in _setup()) {
      step.$2(editor);
    }
    for (final call in _calls) {
      call._replay?.call(editor);
    }
    return editor;
  }

  /// The calls that put a fresh editor where the window starts. Source mode
  /// comes first: there any offset holds the caret, which the live document
  /// the editor opens with would move.
  List<(String, void Function(FlarkEditor))> _setup() => [
    if (_sourceMode)
      ('editor.setSourceMode(true);', (e) => e.setSourceMode(true)),
    if (_sourceMode || !_selection.isCollapsed)
      (
        'editor.apply(${describeCommand(SetSelection(_selection.base, _selection.extent))});',
        (e) => e.apply(SetSelection(_selection.base, _selection.extent)),
      ),
    if (_composing) ('editor.beginComposition();', (e) => e.beginComposition()),
  ];

  /// Dart that rebuilds the editor where the window starts, makes the
  /// recorded calls with the times history used, and expects the source and
  /// selection they ended with. As a regression test it passes until the
  /// behavior changes: correct the expectations to the intended result.
  String get repro {
    final editor = _editor;
    if (editor == null) return '// Nothing was recorded.';
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
    final options = editor._options;
    if (!options.softBreakAsNewline || !options.editableDelimiterRows) {
      out.writeln(
        '  options: const ProjectionOptions('
        '${options.softBreakAsNewline ? '' : 'softBreakAsNewline: false, '}'
        '${options.editableDelimiterRows ? '' : 'editableDelimiterRows: false'}'
        '),',
      );
    }
    out.writeln(');');
    for (final (code, _) in _setup()) {
      out.writeln(code);
    }
    var source = _source;
    var selection = _selection;
    for (final call in _calls) {
      // A result is a line comment: an error's message may hold line breaks.
      final result = call._result
          .replaceAll('\r', r'\r')
          .replaceAll('\n', r'\n');
      out.writeln(
        call._replay == null
            ? '// ${call._code}; // $result'
            : '${call._code}; // $result',
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

/// One recorded call: the Dart that makes it, what it returned, how to make
/// it again (null when a replay must not, as for a call refused for a stale
/// revision, which a replay's revisions would not reproduce), and the change
/// it made with the state it left.
final class _RecordedCall {
  _RecordedCall(this._code, this._result, this._replay);

  final String _code, _result;
  final void Function(FlarkEditor)? _replay;
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
