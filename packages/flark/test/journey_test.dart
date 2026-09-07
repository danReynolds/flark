/// Direct editing scenarios for the V5 kernel.
///
/// Each named test below uses production [FlarkCommand] values. There is no
/// serialized scenario vocabulary to decode or keep in sync with the kernel.
library;

import 'package:flark/flark.dart';
import 'package:test/test.dart';

import 'support/invariants.dart';

part 'journeys/history_cases.dart';
part 'journeys/inline_cases.dart';
part 'journeys/navigation_cases.dart';
part 'journeys/structure_cases.dart';

final class _Session {
  _Session(FlarkParseBackend backend, {String source = '', int caret = 0})
    : editor = FlarkEditor(backend, text: source, caret: caret) {
    _check('load');
  }

  final FlarkEditor editor;
  var _time = Duration.zero;

  void expectState({
    String? source,
    List<String>? rows,
    DisplayPosition? caret,
    int? anchor,
    FlarkSelection? selection,
    int? context,
  }) {
    if (source != null) expect(editor.source, source, reason: 'source');
    if (rows != null) {
      expect(
        editor.projection.rows.map((row) => row.text).toList(),
        rows,
        reason: 'rows',
      );
    }
    if (caret != null) {
      final actual = editor.document.displayOf(editor.selection.extent);
      expect(
        (actual.row, actual.offset),
        (caret.row, caret.offset),
        reason: 'caret (anchor ${editor.selection.extent})',
      );
    }
    if (anchor != null) {
      expect(editor.selection.extent, anchor, reason: 'anchor');
    }
    if (selection != null) {
      expect(editor.selection, selection, reason: 'selection');
    }
    if (context != null) {
      expect(editor.typingContext, context, reason: 'typing context');
    }
    _check('state');
  }

  void act(
    FlarkCommand command, {
    int times = 1,
    int afterMilliseconds = 100,
    bool applied = true,
    String? source,
    List<String>? rows,
    DisplayPosition? caret,
    int? anchor,
    FlarkSelection? selection,
    int? context,
  }) {
    _time += Duration(milliseconds: afterMilliseconds);
    for (var i = 0; i < times; i++) {
      expect(
        editor.apply(command, at: _time),
        applied,
        reason: '${command.runtimeType} repetition ${i + 1} applied',
      );
      _check('${command.runtimeType} repetition ${i + 1}');
    }
    expectState(
      source: source,
      rows: rows,
      caret: caret,
      anchor: anchor,
      selection: selection,
      context: context,
    );
  }

  void _check(String label) {
    final document = editor.document;
    expect(
      (!document.selection.isCollapsed &&
              document.selection.start == 0 &&
              document.selection.end == document.source.length) ||
          document.isLegal(document.selection.base),
      isTrue,
      reason: '$label: legal selection base',
    );
    expect(
      (!document.selection.isCollapsed &&
              document.selection.start == 0 &&
              document.selection.end == document.source.length) ||
          document.isLegal(document.selection.extent),
      isTrue,
      reason: '$label: legal selection extent',
    );
    checkInvariants(
      document.source,
      document.model,
      document.projection,
      label,
    );
  }
}

void main() {
  final backend = createParseBackend();
  _historyCases(backend);
  _inlineCases(backend);
  _navigationCases(backend);
  _structureCases(backend);
}
