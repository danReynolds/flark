/// Journeys: the visible transcript of an editing session, step by step.
/// Each fixture under test/journeys/ is a list of journeys; each step names
/// one command and the expectations after it. Every step also checks the
/// kernel invariants and that undo restores the state the group began in.
library;

import 'dart:convert';
import 'dart:io';

import 'package:flark/flark.dart';
import 'package:test/test.dart';

import 'support/invariants.dart';

const _styleNames = {'emphasis': Style.emphasis, 'strong': Style.strong, 'code': Style.code, 'strikethrough': Style.strikethrough, 'link': Style.link, 'image': Style.image, 'footnoteRef': Style.footnoteRef, 'htmlInline': Style.htmlInline};

List<FlarkCommand> commandsOf(Map<String, Object?> step) {
  int count(Object? v) => v is num ? v.toInt() : 1;
  List<FlarkCommand> repeat(Object? v, FlarkCommand c) => List.filled(count(v), c);
  if (step.containsKey('insert')) return [InsertText(step['insert'] as String)];
  if (step.containsKey('paste')) return [Paste(step['paste'] as String)];
  if (step.containsKey('backspace')) return repeat(step['backspace'], const DeleteBackward());
  if (step.containsKey('delete')) return repeat(step['delete'], const DeleteForward());
  if (step.containsKey('newline')) return repeat(step['newline'], const Newline());
  if (step.containsKey('paragraph')) return repeat(step['paragraph'], const Newline(paragraph: true));
  if (step.containsKey('left')) return repeat(step['left'], const MoveCaret(MoveDirection.backward));
  if (step.containsKey('right')) return repeat(step['right'], const MoveCaret(MoveDirection.forward));
  if (step.containsKey('shiftLeft')) return repeat(step['shiftLeft'], const MoveCaret(MoveDirection.backward, extend: true));
  if (step.containsKey('shiftRight')) return repeat(step['shiftRight'], const MoveCaret(MoveDirection.forward, extend: true));
  if (step.containsKey('wordLeft')) return repeat(step['wordLeft'], const MoveCaret(MoveDirection.backward, unit: MoveUnit.word));
  if (step.containsKey('wordRight')) return repeat(step['wordRight'], const MoveCaret(MoveDirection.forward, unit: MoveUnit.word));
  if (step.containsKey('up')) return repeat(step['up'], const MoveCaret(MoveDirection.backward, unit: MoveUnit.row));
  if (step.containsKey('down')) return repeat(step['down'], const MoveCaret(MoveDirection.forward, unit: MoveUnit.row));
  if (step.containsKey('home')) return const [MoveCaret(MoveDirection.backward, unit: MoveUnit.line)];
  if (step.containsKey('end')) return const [MoveCaret(MoveDirection.forward, unit: MoveUnit.line)];
  if (step.containsKey('select')) { final s = (step['select'] as List).cast<num>(); return [SetSelection(s[0].toInt(), s[1].toInt())]; }
  if (step['caret'] is num) return [SetSelection.caret((step['caret'] as num).toInt())];
  if (step.containsKey('place')) { final p = step['place'] as List; return [PlaceCaret((p[0] as num).toInt(), (p[1] as num).toInt(), leadingHalf: p.length < 3 || p[2] == 'leading')]; }
  if (step.containsKey('replace')) { final r = step['replace'] as List; return [ReplaceRange((r[0] as num).toInt(), (r[1] as num).toInt(), r[2] as String)]; }
  if (step.containsKey('undo')) return repeat(step['undo'], const Undo());
  if (step.containsKey('redo')) return repeat(step['redo'], const Redo());
  if (step.containsKey('toggleTask')) return const [ToggleTask()];
  if (step.containsKey('indent')) return const [Indent()];
  if (step.containsKey('outdent')) return const [Outdent()];
  if (step.containsKey('toggle')) return [ToggleStyle(_styleNames[step['toggle']]!)];
  if (step.containsKey('heading')) return [SetHeadingLevel((step['heading'] as num).toInt())];
  return const [];
}

String describeContext(int mask) => [for (final e in _styleNames.entries) if (mask & e.value != 0) e.key].join(',');

void expectStep(Map<String, Object?> step, FlarkEditor editor, String label) {
  if (step.containsKey('rows')) expect(editor.projection.rows.map((r) => r.text).toList(), (step['rows'] as List).cast<String>(), reason: '$label: rows');
  if (step.containsKey('source')) expect(editor.source, step['source'], reason: '$label: source');
  if (step['caret'] is List) {
    final c = (step['caret'] as List).cast<num>();
    final d = editor.document.displayOf(editor.selection.extent);
    expect([d.row, d.offset], [c[0].toInt(), c[1].toInt()], reason: '$label: caret (anchor ${editor.selection.extent})');
  }
  if (step.containsKey('anchor')) expect(editor.selection.extent, step['anchor'], reason: '$label: anchor');
  if (step.containsKey('selection')) { final s = (step['selection'] as List).cast<num>(); expect(editor.selection, FlarkSelection(s[0].toInt(), s[1].toInt()), reason: '$label: selection'); }
  if (step.containsKey('context')) expect(describeContext(editor.typingContext), (step['context'] as List).cast<String>().join(','), reason: '$label: typing context');
}

void checkStepInvariants(FlarkEditor editor, String label) {
  final doc = editor.document;
  expect(doc.isLegal(doc.selection.base), isTrue, reason: '$label: selection base ${doc.selection.base} is legal');
  expect(doc.isLegal(doc.selection.extent), isTrue, reason: '$label: selection extent ${doc.selection.extent} is legal');
  checkInvariants(doc.source, doc.model, doc.projection, label);
}

void runJourney(Map<String, Object?> j, FlarkParseBackend backend) {
  final editor = FlarkEditor(backend, text: j['source'] as String? ?? '', caret: (j['caret'] as num?)?.toInt() ?? 0);
  final name = j['name'];
  checkStepInvariants(editor, '$name: load');
  var t = Duration.zero;
  var groupStart = (editor.source, editor.selection);
  final steps = (j['steps'] as List).cast<Map<String, Object?>>();
  for (var i = 0; i < steps.length; i++) {
    final step = steps[i];
    t += Duration(milliseconds: (step['after'] as num?)?.toInt() ?? 100);
    final commands = commandsOf(step);
    final label = '$name step ${i + 1} ${commands.isEmpty ? '' : commands.first.runtimeType}';
    final before = (editor.source, editor.selection);
    var applied = commands.isEmpty;
    for (final c in commands) {
      final groupBefore = editor.history.lastGroup;
      final sourceBefore = editor.source, selectionBefore = editor.selection;
      applied = editor.apply(c, at: t) || applied;
      if (editor.source != sourceBefore && editor.history.lastGroup != groupBefore) groupStart = (sourceBefore, selectionBefore);
      if (c is Undo || c is Redo) groupStart = (editor.source, editor.selection);
    }
    expect(applied, step['applied'] ?? true, reason: '$label: applied');
    expectStep(step, editor, label);
    checkStepInvariants(editor, label);
    if (commands.isNotEmpty && commands.first is! Undo && commands.first is! Redo && editor.source != before.$1) {
      // Undo restores the state the open group began in; redo the result.
      final after = (editor.source, editor.selection);
      expect(editor.apply(const Undo(), at: t), isTrue, reason: '$label: undo applies');
      expect((editor.source, editor.selection), groupStart, reason: '$label: undo restores the group start');
      expect(editor.apply(const Redo(), at: t), isTrue, reason: '$label: redo applies');
      expect((editor.source, editor.selection), after, reason: '$label: redo restores the result');
    }
  }
}

void main() {
  final backend = createParseBackend();
  final files = Directory('test/journeys').listSync().whereType<File>().where((f) => f.path.endsWith('.json')).toList()..sort((a, b) => a.path.compareTo(b.path));
  for (final f in files) {
    final journeys = (jsonDecode(f.readAsStringSync()) as List).cast<Map<String, Object?>>();
    group(f.uri.pathSegments.last, () {
      for (final j in journeys) { test(j['name'] as String, () => runJourney(j, backend)); }
    });
  }
}
