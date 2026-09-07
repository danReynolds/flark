import 'package:flark_flutter/flark_flutter.dart';
import 'package:flark_flutter/src/source_window.dart';
import 'package:flark_flutter/src/surface.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets(
    'source inspection stays bounded and follows the canonical caret',
    (tester) async {
      final source = 'abcdefghij' * 24000;
      final c = FlarkController(
        FlarkEditor(createParseBackend(), text: source, caret: 0),
      );
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(body: FlarkSourceView(controller: c)),
        ),
      );
      final first = tester
          .widget<SelectableText>(find.byType(SelectableText))
          .data!;
      expect(first, source.substring(0, 4096));
      expect(find.textContaining('Source page 1 of'), findsOneWidget);
      c.command(SetSelection.caret(source.length));
      await tester.pump();
      final page = SourceWindow.at(source, source.length);
      expect(
        tester.widget<SelectableText>(find.byType(SelectableText)).data,
        source.substring(page.start),
      );
      expect(c.text, source);
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );
  test(
    'source windows preserve every code unit including CRLF and surrogate pairs',
    () {
      for (final source in [
        'x' * 20000,
        '\n' * 20000,
        ('a\r\n😀' * 10000),
        ('e\u0301' * 10000),
        ('👩‍👩‍👧‍👦' * 1000),
      ]) {
        var start = 0;
        final parts = <String>[];
        do {
          final page = SourceWindow.at(source, start);
          final part = source.substring(page.start, page.end);
          expect(page.start, start);
          expect(part.length, lessThanOrEqualTo(4097));
          expect(part.split('\n').length, lessThanOrEqualTo(129));
          expect(
            part.startsWith('\n') &&
                page.start > 0 &&
                source.codeUnitAt(page.start - 1) == 13,
            isFalse,
          );
          if (part.isNotEmpty) {
            expect(
              part.codeUnitAt(0) >= 0xdc00 && part.codeUnitAt(0) <= 0xdfff,
              isFalse,
            );
          }
          parts.add(part);
          start = page.end;
        } while (start < source.length);
        expect(parts.join(), source);
      }
    },
  );
  testWidgets('large source pages stay bounded and edits keep global offsets', (
    tester,
  ) async {
    final source = '\n' * 20000;
    final c = FlarkController(
      FlarkEditor(createParseBackend(), text: source, caret: source.length),
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
    expect(c.editor.sourceMode, isTrue);
    final surface = tester.renderObject<RenderFlarkSurface>(
      find.byType(FlarkSurface),
    );
    expect(surface.size.height, lessThan(6000));
    expect(find.textContaining('Source page'), findsOneWidget);
    expect(c.command(const InsertText('x')), isTrue);
    await tester.pump();
    expect(c.text, '${source}x');
    expect(paints.last.caretSource, source.length + 1);
    await tester.tap(find.byTooltip('Previous source page'));
    await tester.pump();
    final target = c.editor.selection.extent;
    expect(target, lessThan(source.length));
    expect(c.command(const InsertText('y')), isTrue);
    await tester.pump();
    expect(c.text, "${source.replaceRange(target, target, 'y')}x");
    expect(c.command(const Undo()), isTrue);
    await tester.pump();
    expect(c.text, '${source}x');
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  });
}
