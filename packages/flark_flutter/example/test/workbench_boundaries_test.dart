import 'dart:convert';
import 'dart:ui' show AppExitResponse;

import 'package:flark_dogfood/main.dart';
import 'package:flark_dogfood/qualification.dart';
import 'package:flark_flutter/flark_flutter.dart';
// ignore: implementation_imports
import 'package:flark_flutter/src/source_window.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../integration_test/workbench_profile_test.dart'
    show workbenchProfileSources, workbenchOpeningCaret;

void main() {
  final backend = createParseBackend();
  final sources = workbenchProfileSources(backend);
  for (final entry in sources.entries) {
    testWidgets('workbench ${entry.key}: first paint, next input and save', (
      tester,
    ) async {
      final source = entry.value;
      final live = profileCycles.containsKey(entry.key);
      expect(utf8.encode(source).length, lessThan(candidateSourceBytes));
      if (live) expect(utf8.encode(source).length, candidateLiveBytes - 1);
      SharedPreferences.setMockInitialValues({
        'v5.active': 'Tour',
        'v5.source.Tour': source,
      });
      final preferences = await SharedPreferences.getInstance();
      final paints = <FlarkPaintObservation>[];
      await tester.pumpWidget(
        DogfoodApp(
          backend: backend,
          preferences: preferences,
          onPaint: paints.add,
        ),
      );
      await tester.pump();
      final c = tester
          .widget<FlarkEditorWidget>(find.byType(FlarkEditorWidget))
          .controller;
      expect(c.editor.sourceMode, !live);
      expect(
        c.editor.selection,
        FlarkSelection.collapsed(workbenchOpeningCaret(entry.key)),
      );
      expect(paints.every((p) => p.snapshot.source == source), isTrue);
      expect(paints.last.caret, isNotNull);
      c.command(SetSelection.caret(source.length));
      await tester.pump();

      Future<void> edit(
        FlarkCommand command,
        String expected,
        bool fallback,
      ) async {
        paints.clear();
        expect(c.command(command), isTrue);
        await tester.pump();
        expect(tester.takeException(), isNull);
        expect(c.text, expected);
        expect(c.editor.sourceMode, fallback);
        expect(c.editor.selection.extent, expected.length);
        expect(paints, isNotEmpty);
        for (final p in paints) {
          expect(p.snapshot.source, expected);
          expect(p.caretSource, expected.length);
          expect(p.caret, isNotNull);
          final lineStart = expected.lastIndexOf('\n') + 1;
          var tailStart = expected.length - 32;
          if (fallback) {
            final page = SourceWindow.at(expected, expected.length);
            if (page.start > tailStart) tailStart = page.start;
            expect(
              find.text('Source page ${page.index + 1} of ${page.count}'),
              page.count > 1 ? findsOneWidget : findsNothing,
            );
          }
          expect(
            p.rows.last,
            endsWith(
              expected.substring(tailStart > lineStart ? tailStart : lineStart),
            ),
          );
        }
      }

      await edit(const InsertText('x'), '${source}x', !live);
      if (live) {
        await edit(const InsertText('y'), '${source}xy', true);
        c.command(SetSelection.caret(source.length + 1));
        c.command(SetSelection.caret(source.length + 2));
        await tester.pump();
        await edit(const DeleteBackward(), '${source}x', false);
        await edit(const Undo(), '${source}xy', true);
        await edit(const Redo(), '${source}x', false);
      }
      await edit(const DeleteBackward(), source, !live);
      for (var toggle = 0; toggle < 2; toggle++) {
        paints.clear();
        await tester.tap(find.byTooltip('Inspect Markdown'));
        await tester.pump();
        expect(c.text, source);
        expect(c.editor.selection.extent, source.length);
        expect(paints, isNotEmpty);
        expect(paints.first.caret, isNotNull);
      }
      expect(await tester.binding.handleRequestAppExit(), AppExitResponse.exit);
      expect(preferences.getString('v5.source.Tour'), source);
      await tester.pumpWidget(const SizedBox());
    });
  }
}
