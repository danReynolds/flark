import 'dart:ui' show SemanticsAction, SemanticsActionEvent;
import 'package:flutter/gestures.dart';
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flark_flutter/src/surface.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();
  for (final suffix in ['\n', '\n\nAfter']) {
    testWidgets(
      'table exit with existing suffix paints the next input: $suffix',
      (tester) async {
        const table = '| a | b |\n| - | - |\n| c | d |';
        final c = FlarkController(
          FlarkEditor(
            backend,
            text: '$table$suffix',
            caret: table.lastIndexOf('d'),
          ),
        );
        final paints = <FlarkPaintObservation>[];
        await tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: FlarkEditorWidget(
                controller: c,
                autofocus: true,
                onPaint: paints.add,
              ),
            ),
          ),
        );
        await tester.pump();
        paints.clear();
        expect(c.command(const Newline()), isTrue);
        expect(c.command(const InsertText('x')), isTrue);
        await tester.pump();
        for (final p in paints) {
          expect(
            p.rows.map((r) => r.trim()),
            containsAllInOrder(['a', 'b', 'c', 'd', '', 'x']),
          );
          expect(p.caret, isNotNull);
          expect(p.snapshot.source, '$table\n\nx$suffix');
        }
        expect(paints, isNotEmpty);
        await tester.pumpWidget(const SizedBox());
        c.dispose();
      },
    );
  }

  testWidgets(
    'byte crossing and history switch presentation in the first frame',
    (tester) async {
      final c = FlarkController(
        FlarkEditor(backend, text: '**a**', caret: 3, syncLimit: 6),
      );
      final paints = <FlarkPaintObservation>[];
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(
              controller: c,
              autofocus: true,
              onPaint: paints.add,
            ),
          ),
        ),
      );
      await tester.pump();
      for (final (command, source, visible, raw)
          in <(FlarkCommand, String, String, bool)>[
            (const InsertText('xx'), '**axx**', '**axx**', true),
            (const Undo(), '**a**', 'a', false),
            (const Redo(), '**axx**', '**axx**', true),
            (const DeleteBackward(), '**ax**', 'ax', false),
          ]) {
        paints.clear();
        expect(c.command(command), isTrue);
        await tester.pump();
        expect(paints, isNotEmpty);
        for (final p in paints) {
          expect(p.snapshot.source, source);
          expect(p.rows, [visible]);
          expect(p.snapshot is FlarkSourceSnapshot, raw);
          expect(p.caret, isNotNull);
          expect(p.revision, c.editor.revision);
        }
      }
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  testWidgets(
    'pointer placement preserves distinct typing contexts at one glyph edge',
    (tester) async {
      final c = FlarkController(FlarkEditor(backend, text: '*ab*', caret: 0));
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(controller: c, autofocus: true),
          ),
        ),
      );
      await tester.pump();
      final surface = tester.renderObject<RenderFlarkSurface>(
        find.byType(FlarkSurface),
      );
      final edge = surface.caretRect;
      for (final inside in [false, true]) {
        await tester.tapAt(
          surface.localToGlobal(
            Offset(edge.left + (inside ? 1 : -1), edge.center.dy),
          ),
        );
        expect(c.editor.typingContext, inside ? Style.emphasis : 0);
        expect(c.command(const InsertText('x')), isTrue);
        expect(c.text, inside ? '*xab*' : 'x*ab*');
        await tester.pump();
        expect(c.command(const Undo()), isTrue);
        await tester.pump();
        expect(c.text, '*ab*');
      }
      await tester.pumpWidget(const SizedBox());
      await tester.pump(kDoubleTapMinTime);
      c.dispose();
    },
  );

  testWidgets(
    'accessibility selection maps visible offsets into canonical source',
    (tester) async {
      final semantics = tester.ensureSemantics();
      final c = FlarkController(
        FlarkEditor(backend, text: '**ab** cd', caret: 2),
      );
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(controller: c, autofocus: true),
          ),
        ),
      );
      await tester.pump();
      final node = tester.getSemantics(find.byType(FlarkSurface));
      expect(node.getSemanticsData().value, 'ab cd');
      for (final (base, extent, visible, result) in [
        (1, 2, 'b', '**ax** cd'),
        (2, 1, 'b', '**ax** cd'),
        (0, 2, 'ab', '**x** cd'),
      ]) {
        tester.binding.performSemanticsAction(
          SemanticsActionEvent(
            viewId: tester.view.viewId,
            nodeId: node.id,
            type: SemanticsAction.setSelection,
            arguments: {'base': base, 'extent': extent},
          ),
        );
        await tester.pump();
        expect(c.selectedText, visible);
        expect(c.command(const InsertText('x')), isTrue);
        await tester.pump();
        expect(
          node.getSemanticsData().value,
          result == '**x** cd' ? 'x cd' : 'ax cd',
        );
        expect(c.text, result);
        expect(c.command(const Undo()), isTrue);
        await tester.pump();
      }
      await tester.pumpWidget(const SizedBox());
      c.dispose();
      semantics.dispose();
    },
  );
}
