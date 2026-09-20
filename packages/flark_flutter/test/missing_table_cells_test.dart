import 'dart:ui' show SemanticsAction;

import 'package:flark_flutter/flark_flutter.dart';
import 'package:flark_flutter/src/surface.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  for (final width in [240.0, 800.0]) {
    testWidgets(
      'missing cells support pointer, Tab, platform typing and undo at $width',
      (t) async {
        final semantics = t.ensureSemantics();
        const source = '| a | b | c |\n| --- | --- | --- |\n| x |\n';
        final e = FlarkEditor(createParseBackend(), text: source);
        final c = FlarkController(e);
        final paints = <FlarkPaintObservation>[];
        await t.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: SizedBox(
                width: width,
                child: FlarkEditorWidget(
                  controller: c,
                  autofocus: true,
                  showToolbar: false,
                  onPaint: paints.add,
                ),
              ),
            ),
          ),
        );
        await t.pump();
        final surface = t.renderObject<RenderFlarkSurface>(
          find.byType(FlarkSurface),
        );
        c.command(SetSelection.caret(source.indexOf('b')));
        await t.pump();
        final x = surface.caretRect.center.dx;
        c.command(SetSelection.caret(source.indexOf('x')));
        await t.pump();
        final y = surface.caretRect.center.dy;
        await t.tapAt(surface.localToGlobal(Offset(x, y)));
        await t.pump();
        expect(e.document.caretRow.column, 1);
        expect(e.source, source);
        expect(surface.caretRect.center.dx, closeTo(x, 1));
        await t.sendKeyEvent(LogicalKeyboardKey.tab);
        await t.pump();
        expect(e.document.caretRow.column, 2);
        final emptyCaret = surface.caretRect;
        final node = t.getSemantics(find.byType(FlarkSurface));
        final displayedSelection = node.getSemanticsData().textSelection!;
        c.command(SetSelection.caret(source.indexOf('x')));
        await t.pump();
        node.owner!.performAction(node.id, SemanticsAction.setSelection, {
          'base': displayedSelection.baseOffset,
          'extent': displayedSelection.extentOffset,
        });
        await t.pump();
        expect(e.document.caretRow.column, 2);
        expect(surface.caretRect, emptyCaret);
        paints.clear();
        final selection = e.selection;
        t.testTextInput.updateEditingValue(
          TextEditingValue(
            text: e.source.replaceRange(selection.start, selection.end, 'Z'),
            selection: TextSelection.collapsed(offset: selection.start + 1),
          ),
        );
        await t.pump();
        expect(e.document.caretRow.column, 2);
        expect(e.document.caretRow.text.trim(), 'Z');
        expect(paints, isNotEmpty);
        for (final p in paints) {
          expect(
            p.rows.map((s) => s.trim()),
            containsAllInOrder(['x', '', 'Z']),
          );
          expect(p.caret!.left, greaterThanOrEqualTo(emptyCaret.left));
          expect(p.caret!.top, emptyCaret.top);
        }
        c.command(const Undo());
        await t.pump();
        expect(e.source, source);
        expect(surface.caretRect, emptyCaret);
        await t.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
        await t.sendKeyEvent(LogicalKeyboardKey.tab);
        await t.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
        expect(e.document.caretRow.column, 1);
        await t.pumpWidget(const SizedBox());
        c.dispose();
        await t.pump(const Duration(milliseconds: 350));
        semantics.dispose();
      },
    );
  }
}
