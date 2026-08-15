import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flark_flutter/flark_flutter_advanced.dart';

import '../support/flark_test_paths.dart';

/// RFC 022 Phase 3 differential oracle: the read-only preview and the live
/// editor derive their rendered text from the same parse through the same
/// segmentation — so for documents whose display is pure text (no cards or
/// interactive chrome), the two surfaces must render character-identical
/// text. Historically they used parallel implementations and drifted (the
/// preview's flat span walk duplicated nested-run text); this oracle makes
/// that class of drift a test failure rather than a review finding.
void main() {
  final libPath = flarkNativeBridgeLibraryPathForPlatform();
  if (libPath.isEmpty || !File(libPath).existsSync()) {
    test('native bridge not built; render parity suite skipped', () {
      expect(true, isTrue);
    });
    return;
  }

  // Deliberately scoped to documents whose block chrome contributes zero
  // CHARACTERS in both surfaces. Excluded classes render character-bearing
  // chrome on one side only and would false-fail: ordered lists (the editor
  // paints a `1.` label as Text), checked task boxes (the preview's box is a
  // `✓` Text, the editor's a CustomPaint), fenced code (editor language
  // badge), tables, and images (preview card labels). Unordered bullets and
  // quote rails are paint-only in both surfaces and stay in scope.
  const corpus = <String>[
    'plain paragraph text',
    'nested ***bold italic*** emphasis',
    'stacked ~~**strike bold**~~ runs',
    'a **bold [link](https://example.com) label** inline',
    'code span `with **literal** markers`',
    'an *italic* then **bold** then `code` sequence',
    'autolink <https://example.com> hides brackets',
    'shortcut [ref] link\n\n[ref]: /url',
    'abutting closers **foo *bar***',
    'fused cluster ***foobar***',
    '# Heading with **bold**\n\nBody with *em*.',
    '> quoted ~~strike~~ text',
    '- item with `code`\n- item with **bold**',
    'escaped \\*not emphasis\\* text',
    'entity &amp; replacement',
  ];

  for (final markdown in corpus) {
    testWidgets('surfaces agree: ${markdown.split('\n').first}', (
      tester,
    ) async {
      final backend = FlarkNativeComrakParseBackend.withNativeBridge(
        overrideLibraryPath: libPath,
      );

      final previewController = FlarkFlutterController.fromMarkdown(
        markdown,
        extensions: FlarkMarkdownEditingExtensions.standard(),
        parseBackend: backend,
      );
      addTearDown(previewController.dispose);
      expect(previewController.tryParseSync(), isTrue);
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(body: FlarkMarkdown(controller: previewController)),
        ),
      );
      final previewText = _visibleText(tester);

      final editorController = FlarkFlutterController.fromMarkdown(
        markdown,
        extensions: FlarkMarkdownEditingExtensions.standard(),
        parseBackend: backend,
      );
      addTearDown(editorController.dispose);
      expect(editorController.tryParseSync(), isTrue);
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FlarkMarkdownEditor(
              controller: editorController,
              editingMode: FlarkMarkdownEditingMode.liveRendered,
            ),
          ),
        ),
      );
      await tester.pump();
      final editorText = _visibleText(tester);

      expect(
        editorText,
        previewText,
        reason:
            'the live editor and the read-only preview must render '
            'identical text for the same document (shared segmentation, '
            'RFC 022 Phase 3)',
      );
    });
  }
}

/// Canonical form of every rendered character across the surface.
///
/// The preview hosts link text inside WidgetSpans (menu wrappers), so a
/// flattened walk yields object-replacement placeholders with the link text
/// collected out of order relative to the editor's inline layout. The drift
/// classes this oracle exists to catch — duplicated segments (`boldbold`),
/// omitted content, leaked marker characters — all change WHICH characters
/// render, never merely their collection order, so the comparison is a
/// whitespace-normalized character multiset: placeholder- and order-immune,
/// unforgiving of content differences.
String _visibleText(WidgetTester tester) {
  final pieces = <String>[];
  for (final widget in tester.widgetList(
    find.byWidgetPredicate(
      (widget) => widget is RichText || widget is EditableText,
    ),
  )) {
    if (widget is RichText) {
      pieces.add(widget.text.toPlainText());
    } else if (widget is EditableText) {
      pieces.add(widget.controller.text);
    }
  }
  final characters =
      pieces.join().replaceAll('￼', '').replaceAll(RegExp(r'\s'), '').split('')
        ..sort();
  return characters.join();
}
