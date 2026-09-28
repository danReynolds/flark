// The editing scenarios in tool/edit_cases.dart, run through the delegate.
import 'package:flark/code.dart';
import 'package:flark_codemirror/flark_codemirror.dart';
import 'package:test/test.dart';

import '../tool/edit_cases.dart';

({String source, int base, int extent}) marked(String value) {
  final caret = value.indexOf('\u00a6');
  if (caret >= 0) {
    return (
      source: value.replaceFirst('\u00a6', ''),
      base: caret,
      extent: caret,
    );
  }
  final base = value.indexOf('\u00ab'), extent = value.indexOf('\u00bb');
  return (
    source: value.replaceAll('\u00ab', '').replaceAll('\u00bb', ''),
    base: base - (extent < base ? 1 : 0),
    extent: extent - (base < extent ? 1 : 0),
  );
}

String show(String source, int base, int extent) {
  if (base == extent) return source.replaceRange(base, base, '\u00a6');
  final lo = base < extent ? base : extent, hi = base < extent ? extent : base;
  return source.replaceRange(hi, hi, '\u00bb').replaceRange(lo, lo, '\u00ab');
}

void main() {
  final delegate = FlarkCodeMirror();
  CodeEditProposal propose(
    EditCase c,
    String source,
    int base,
    int extent,
    CodeEditingAction action,
    String text,
  ) => delegate.propose(
    source,
    language: c.language,
    base: base,
    extent: extent,
    action: action,
    text: text,
    indentUnit: c.unit,
  )!;
  for (final c in cases) {
    test(c.name, () {
      final before = marked(c.before), after = marked(c.after);
      final edit = propose(
        c,
        before.source,
        before.base,
        before.extent,
        c.action,
        c.text,
      );
      final source = before.source.replaceRange(
        edit.start,
        edit.end,
        edit.text,
      );
      expect(
        show(source, edit.base, edit.extent),
        show(after.source, after.base, after.extent),
      );
      // The next typed character lands at the promised selection.
      final next = propose(c, source, edit.base, edit.extent, type, 'z');
      final lo = edit.base < edit.extent ? edit.base : edit.extent;
      expect(
        source.replaceRange(next.start, next.end, next.text),
        source.replaceRange(
          lo,
          edit.base < edit.extent ? edit.extent : edit.base,
          'z',
        ),
      );
    });
  }
}
