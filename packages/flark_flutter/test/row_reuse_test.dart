import 'package:flark_flutter/code.dart';
import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flark_flutter/src/surface.dart';
import 'package:flark_tree_sitter/flark_tree_sitter.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();

  testWidgets('a row entering a quote takes the quote style', (tester) async {
    final c = FlarkController(
      FlarkEditor(backend, text: 'plain words', caret: 0),
    );
    final paints = <FlarkPaintObservation>[];
    await tester.pumpWidget(
      MaterialApp(
        home: FlarkEditorWidget(
          controller: c,
          autofocus: true,
          showToolbar: false,
          onPaint: paints.add,
          theme: FlarkThemeData(
            styles: const {
              FlarkTextRole.quote: TextStyle(fontStyle: FontStyle.italic),
            },
          ),
        ),
      ),
    );
    await tester.pump();
    expect(
      paints.last.resolvedStyles.single.single.fontStyle,
      isNot(FontStyle.italic),
    );
    // The row keeps its text, kind and segments; only its container changes.
    expect(c.command(const InsertText('>')), isTrue);
    await tester.pump();
    expect(c.text, '>plain words');
    expect(paints.last.rows, ['plain words']);
    expect(
      paints.last.resolvedStyles.single.single.fontStyle,
      FontStyle.italic,
    );
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  });

  testWidgets(
    'row-count changes near the top reuse the layouts that only moved',
    (tester) async {
      final text = StringBuffer();
      for (var i = 0; text.length < 30 * 1024; i++) {
        text.write('Paragraph $i has *some* words and a **bold** bit.\n\n');
        if (i % 10 == 0) text.write('- item $i\n- item ${i + 1}\n\n');
      }
      final editor = FlarkEditor(
        backend,
        text: text.toString(),
        caret: 10,
        syncLimit: 64 * 1024,
        sourceLimit: 256 * 1024,
        liveLimits: const FlarkLiveLimits(
          lines: 8192,
          blocks: 8192,
          runs: 16384,
        ),
      );
      final c = FlarkController(editor);
      final paints = <FlarkPaintObservation>[];
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SizedBox(
              width: 800,
              height: 600,
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
      await tester.pump();
      expect(editor.sourceMode, isFalse);
      expect(editor.projection.rows.length, greaterThan(1000));

      void expectPaintedTop() {
        final painted = paints.last.rows;
        expect(painted, isNotEmpty);
        expect(painted, [
          for (final row in editor.projection.rows.take(painted.length))
            row.text,
        ]);
      }

      Future<int> shaped(FlarkCommand command) async {
        final before = RenderFlarkSurface.shapedRows;
        expect(c.command(command), isTrue);
        await tester.pump();
        expectPaintedTop();
        return RenderFlarkSurface.shapedRows - before;
      }

      expect(await shaped(const InsertText('x')), 1);
      // One paragraph becomes two separated by a blank row.
      expect(
        await shaped(const Newline(paragraph: true)),
        lessThanOrEqualTo(3),
      );
      expect(await shaped(const DeleteBackward()), lessThanOrEqualTo(3));
      expect(await shaped(const Undo()), lessThanOrEqualTo(3));
      expect(await shaped(const Redo()), lessThanOrEqualTo(3));

      // Layouts reused after the edit belong to the rows now painted there.
      final scroll = tester.state<ScrollableState>(
        find.byType(Scrollable).last,
      );
      scroll.position.jumpTo(scroll.position.maxScrollExtent);
      await tester.pump();
      final painted = paints.last.rows;
      final rows = editor.projection.rows;
      expect(painted, [
        for (final row in rows.skip(rows.length - painted.length)) row.text,
      ]);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );

  testWidgets('scrolling reports newly visible fences without a rebuild', (
    tester,
  ) async {
    final text = StringBuffer();
    for (var i = 0; i < 60; i++) {
      text.write('Paragraph $i\n\n```dart\nfinal fence$i = $i;\n```\n\n');
    }
    final editor = FlarkEditor(backend, text: text.toString(), caret: 0);
    final requested = <String>{};
    final colors = FlarkCodeColors.withWorker(editor, (
      source, {
      required CodeLanguage language,
    }) async {
      requested.add(source);
      return null;
    }, () {});
    final c = FlarkController(editor, codeColors: colors);
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 800,
            height: 600,
            child: FlarkEditorWidget(controller: c, showToolbar: false),
          ),
        ),
      ),
    );
    await tester.pump();
    expect(requested, isNot(contains('final fence59 = 59;')));
    final scroll = tester.state<ScrollableState>(find.byType(Scrollable).last);
    scroll.position.jumpTo(scroll.position.maxScrollExtent);
    await tester.pump();
    await tester.pump();
    expect(requested, contains('final fence59 = 59;'));
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  });
}
