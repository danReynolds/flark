/// The bounds a [CodeEditingDelegate]'s proposal must keep, as its
/// documentation states them: a proposal outside them is a delegate error,
/// and the command that asked for it throws a [StateError] before it changes
/// anything. A carriage return in a proposal is invalid source, refused.
library;

import 'package:flark/code.dart';
import 'package:flark/flark.dart';
import 'package:test/test.dart';

/// Proposes [proposal] for every request, in a language it calls `x`.
final class _Proposing implements CodeEditingDelegate {
  _Proposing(this.proposal);

  final CodeEditProposal proposal;

  @override
  String resolveLanguage(String source, String info) => 'x';

  @override
  CodeHighlight highlight(String source, String info) =>
      CodeHighlight(null, [CodeToken(0, source.length, null)]);

  @override
  CodeEditProposal? propose(
    String source, {
    required String language,
    required int base,
    required int extent,
    required CodeEditingAction action,
    required String text,
    required String indentUnit,
  }) => proposal;
}

void main() {
  final backend = createParseBackend();
  // The body is `ab\ncd`: five code units on two lines.
  const source = '```x\nab\ncd\n```';
  FlarkEditor editor(CodeEditProposal proposal) => FlarkEditor(
    backend,
    text: source,
    caret: source.indexOf('b'),
    codeEditing: _Proposing(proposal),
  );

  test('a proposal outside its bounds throws and changes nothing', () {
    for (final (proposal, command, what) in [
      (const CodeEditProposal(0, 6, 'x', 1, 1), const InsertText('x'), 'end'),
      (const CodeEditProposal(2, 1, 'x', 1, 1), const InsertText('x'), 'range'),
      (const CodeEditProposal(0, 0, 'x', 0, 7), const InsertText('x'), 'caret'),
      (const CodeEditProposal(0, 5, 'ab', 0, 0), const Indent(), 'lines'),
    ]) {
      final e = editor(proposal);
      final snapshot = e.snapshot;
      expect(() => e.apply(command), throwsStateError, reason: what);
      expect(identical(e.snapshot, snapshot), isTrue, reason: what);
      expect(e.history.canUndo, isFalse, reason: what);
    }
  });

  test('a proposal with a carriage return is refused as invalid source', () {
    final e = editor(const CodeEditProposal(2, 2, '\r', 3, 3));
    final snapshot = e.snapshot;
    expect(e.apply(const InsertText('x')), isFalse);
    expect(e.lastRejection, FlarkRejection.invalidSource);
    expect(identical(e.snapshot, snapshot), isTrue);
  });
}
