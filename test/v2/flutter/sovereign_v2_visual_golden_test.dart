import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/sovereign_editor_v2.dart';

void main() {
  setUpAll(() async {
    await _loadGoldenFonts();
  });

  group('Sovereign v2 visual contracts', () {
    testWidgets('overview surfaces stay visually stable', (tester) async {
      await _pumpTriptych(tester, _overviewMarkdown);

      await expectLater(
        find.byKey(_goldenKey),
        matchesGoldenFile('goldens/sovereign_v2_surfaces.png'),
      );
    });

    testWidgets('live rendered editing surface stays visually stable', (
      tester,
    ) async {
      await _pumpLiveRenderedOnly(
        tester,
        _overviewMarkdown,
        width: 460,
        height: 620,
      );

      await expectLater(
        find.byKey(_goldenKey),
        matchesGoldenFile('goldens/sovereign_v2_live_rendered_editing.png'),
      );
    });

    testWidgets('live rendered edge cases stay visually stable', (
      tester,
    ) async {
      await _pumpLiveRenderedEdgeCases(tester, width: 900, height: 760);

      await expectLater(
        find.byKey(_goldenKey),
        matchesGoldenFile('goldens/sovereign_v2_live_edge_cases.png'),
      );
    });

    testWidgets('inline styling and wrapping stay visually stable', (
      tester,
    ) async {
      await _pumpPreviewOnly(
        tester,
        _inlineWrappingMarkdown,
        width: 420,
        height: 520,
      );

      await expectLater(
        find.byKey(_goldenKey),
        matchesGoldenFile('goldens/sovereign_v2_inline_wrapping.png'),
      );
    });

    testWidgets('code fence regions stay visually stable', (tester) async {
      await _pumpTriptych(tester, _codeFenceMarkdown, height: 560);

      await expectLater(
        find.byKey(_goldenKey),
        matchesGoldenFile('goldens/sovereign_v2_code_fences.png'),
      );
    });

    testWidgets('blockquote regions stay visually stable', (tester) async {
      await _pumpTriptych(tester, _blockquoteMarkdown, height: 560);

      await expectLater(
        find.byKey(_goldenKey),
        matchesGoldenFile('goldens/sovereign_v2_blockquotes.png'),
      );
    });

    testWidgets('tasks, tables, and overlays stay visually stable', (
      tester,
    ) async {
      await _pumpTriptych(
        tester,
        _tasksTablesOverlayMarkdown,
        height: 620,
        showOverlayControls: true,
      );

      await expectLater(
        find.byKey(_goldenKey),
        matchesGoldenFile('goldens/sovereign_v2_tasks_tables_overlays.png'),
      );
    });

    testWidgets('compact mixed markdown layout stays visually stable', (
      tester,
    ) async {
      await _pumpPreviewOnly(
        tester,
        _compactMixedMarkdown,
        width: 360,
        height: 640,
        showOverlayControls: true,
      );

      await expectLater(
        find.byKey(_goldenKey),
        matchesGoldenFile('goldens/sovereign_v2_compact_mixed.png'),
      );
    });
  });
}

const _goldenKey = Key('sovereign-v2-visual-golden');

const _overviewMarkdown = '''
# Release plan

A **bold** link to [Docs](https://example.com), image ![Diagram](asset://diagram.png), and `code`.

- [x] Ship editor
- [ ] Add visual guard

> Keep quoted context visible beside the rail.

```dart
final answer = 42;
```

| Area | Status |
| --- | --- |
| Preview | Guarded |''';

const _inlineWrappingMarkdown = '''
# Inline styling

This intentionally long paragraph combines **bold text**, *italic text*, ~~deleted text~~, `inline code`, [a visible link](https://example.com/docs), and escaped \\*literal markers\\* so line wrapping, marker hiding, and inline spans are checked together in a narrow preview.

Final sentence after the wrap.''';

const _codeFenceMarkdown = '''
# Code fences

```dart
final items = <String>['alpha', 'beta', 'gamma'];
print(items.join(', '));
```

Paragraph after the Dart fence.

```
plain fence without a language
keeps a readable block region
```''';

const _blockquoteMarkdown = '''
# Quotes

> A quoted paragraph with **bold emphasis** that should keep a visible rail.
> Continued quote line for vertical rhythm.

> > Nested quote marker text should remain readable after projection.

Paragraph after quotes.''';

const _tasksTablesOverlayMarkdown = '''
# Tracking

- [x] Code fences guarded
- [ ] Quote rail reviewed

Open [Docs](https://example.com/docs) and inspect ![Diagram](asset://diagram.png).

| Area | Status | Owner |
| --- | :---: | ---: |
| Code | guarded | core |
| Quotes | visual | UI |''';

const _compactMixedMarkdown = '''
# Compact

Long **bold** and *italic* text wraps beside [Docs](https://example.com) without clipping.

> A compact quote wraps over multiple lines and keeps the rail aligned.

```dart
final value = "compact";
```

- [x] Done
- [ ] Open''';

const _edgeListExitMarkdown = '''
- item

''';

const _edgeBlankRowsMarkdown = '''
- one


- two''';

const _edgeFencedBeforeQuoteMarkdown = '''
```dart
final value = 1;
```

> quote after fence''';

const _edgeOpenFenceMarkdown = '''
- before

```
open fence
  code''';

const _editorStyle = TextStyle(
  color: Color(0xFF17202A),
  fontFamily: 'Roboto',
  fontSize: 13,
  height: 1.35,
);

Future<void> _pumpTriptych(
  WidgetTester tester,
  String markdown, {
  double width = 960,
  double height = 620,
  bool showOverlayControls = false,
}) async {
  _setGoldenSurface(tester, width, height);
  final sourceController = _controller(markdown);
  final projectedController = _controller(markdown);
  addTearDown(sourceController.dispose);
  addTearDown(projectedController.dispose);

  await tester.pumpWidget(
    _GoldenSurfaceFrame(
      width: width,
      height: height,
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Expanded(
            child: _SurfacePanel(
              label: 'Source editor',
              child: MarkdownEditor(
                controller: sourceController,
                parseBackend: const _GoldenParseBackend(),
                parseDebounce: Duration.zero,
                editingMode: SovereignMarkdownEditingMode.source,
                style: _editorStyle,
                maxLines: 24,
              ),
            ),
          ),
          const SizedBox(width: 12),
          Expanded(
            child: _SurfacePanel(
              label: 'Projected editor',
              child: MarkdownEditor(
                controller: projectedController,
                parseBackend: const _GoldenParseBackend(),
                parseDebounce: Duration.zero,
                editingMode: SovereignMarkdownEditingMode.projected,
                style: _editorStyle,
                maxLines: 24,
              ),
            ),
          ),
          const SizedBox(width: 12),
          Expanded(
            child: _SurfacePanel(
              label: 'Preview and overlays',
              child: Markdown(
                markdown: markdown,
                parseBackend: const _GoldenParseBackend(),
                parseDebounce: Duration.zero,
                textStyle: _editorStyle,
                showOverlayControls: showOverlayControls,
              ),
            ),
          ),
        ],
      ),
    ),
  );
  await tester.pump();
  await tester.pump();
}

Future<void> _pumpPreviewOnly(
  WidgetTester tester,
  String markdown, {
  required double width,
  required double height,
  bool showOverlayControls = false,
}) async {
  _setGoldenSurface(tester, width, height);
  await tester.pumpWidget(
    _GoldenSurfaceFrame(
      width: width,
      height: height,
      child: _SurfacePanel(
        label: 'Preview',
        child: Markdown(
          markdown: markdown,
          parseBackend: const _GoldenParseBackend(),
          parseDebounce: Duration.zero,
          textStyle: _editorStyle,
          showOverlayControls: showOverlayControls,
        ),
      ),
    ),
  );
  await tester.pump();
  await tester.pump();
}

Future<void> _pumpLiveRenderedOnly(
  WidgetTester tester,
  String markdown, {
  required double width,
  required double height,
}) async {
  _setGoldenSurface(tester, width, height);
  final controller = _controller(markdown);
  addTearDown(controller.dispose);

  await tester.pumpWidget(
    _GoldenSurfaceFrame(
      width: width,
      height: height,
      child: _SurfacePanel(
        label: 'Live rendered editor',
        child: MarkdownEditor(
          controller: controller,
          parseBackend: const _GoldenParseBackend(),
          parseDebounce: Duration.zero,
          editingMode: SovereignMarkdownEditingMode.liveRendered,
          style: _editorStyle,
          maxLines: 28,
        ),
      ),
    ),
  );
  await tester.pump();
  await tester.pump();
}

Future<void> _pumpLiveRenderedEdgeCases(
  WidgetTester tester, {
  required double width,
  required double height,
}) async {
  _setGoldenSurface(tester, width, height);
  final cases = [
    _LiveEdgeCase(
      label: 'List exit keeps blank host',
      markdown: _edgeListExitMarkdown,
    ),
    _LiveEdgeCase(
      label: 'Parser-omitted blank rows',
      markdown: _edgeBlankRowsMarkdown,
    ),
    _LiveEdgeCase(
      label: 'Fence bounded before quote',
      markdown: _edgeFencedBeforeQuoteMarkdown,
    ),
    _LiveEdgeCase(
      label: 'Open fence after list gap',
      markdown: _edgeOpenFenceMarkdown,
    ),
  ];

  for (final edgeCase in cases) {
    addTearDown(edgeCase.controller.dispose);
  }

  await tester.pumpWidget(
    _GoldenSurfaceFrame(
      width: width,
      height: height,
      child: Column(
        children: [
          Expanded(
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Expanded(child: _LiveEdgeCasePanel(edgeCase: cases[0])),
                const SizedBox(width: 12),
                Expanded(child: _LiveEdgeCasePanel(edgeCase: cases[1])),
              ],
            ),
          ),
          const SizedBox(height: 12),
          Expanded(
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Expanded(child: _LiveEdgeCasePanel(edgeCase: cases[2])),
                const SizedBox(width: 12),
                Expanded(child: _LiveEdgeCasePanel(edgeCase: cases[3])),
              ],
            ),
          ),
        ],
      ),
    ),
  );
  await tester.pump();
  await tester.pump();
}

void _setGoldenSurface(WidgetTester tester, double width, double height) {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = Size(width, height);
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
}

SovereignFlutterController _controller(String markdown) {
  return SovereignFlutterController.fromMarkdown(
    markdown,
    extensions: SovereignMarkdownEditingExtensions.standard(),
  );
}

final class _GoldenSurfaceFrame extends StatelessWidget {
  const _GoldenSurfaceFrame({
    required this.width,
    required this.height,
    required this.child,
  });

  final double width;
  final double height;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Directionality(
      textDirection: TextDirection.ltr,
      child: RepaintBoundary(
        key: _goldenKey,
        child: ColoredBox(
          color: const Color(0xFFF6F7F9),
          child: SizedBox(
            width: width,
            height: height,
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: DefaultTextStyle(style: _editorStyle, child: child),
            ),
          ),
        ),
      ),
    );
  }
}

final class _SurfacePanel extends StatelessWidget {
  const _SurfacePanel({required this.label, required this.child});

  final String label;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(
        color: const Color(0xFFFFFFFF),
        border: Border.all(color: const Color(0xFFD7DEE8)),
        borderRadius: BorderRadius.circular(6),
      ),
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              label,
              style: const TextStyle(
                color: Color(0xFF42526E),
                fontSize: 11,
                fontWeight: FontWeight.w700,
              ),
            ),
            const SizedBox(height: 8),
            child,
          ],
        ),
      ),
    );
  }
}

final class _LiveEdgeCase {
  _LiveEdgeCase({required this.label, required String markdown})
    : controller = _controller(markdown);

  final String label;
  final SovereignFlutterController controller;
}

final class _LiveEdgeCasePanel extends StatelessWidget {
  const _LiveEdgeCasePanel({required this.edgeCase});

  final _LiveEdgeCase edgeCase;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(
        color: const Color(0xFFFFFFFF),
        border: Border.all(color: const Color(0xFFD7DEE8)),
        borderRadius: BorderRadius.circular(6),
      ),
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              edgeCase.label,
              style: const TextStyle(
                color: Color(0xFF42526E),
                fontSize: 11,
                fontWeight: FontWeight.w700,
              ),
            ),
            const SizedBox(height: 8),
            Expanded(
              child: MarkdownEditor(
                controller: edgeCase.controller,
                parseBackend: const _GoldenParseBackend(),
                parseDebounce: Duration.zero,
                editingMode: SovereignMarkdownEditingMode.liveRendered,
                style: _editorStyle,
                expands: true,
                maxLines: null,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

final class _GoldenParseBackend implements SovereignMarkdownParseBackend {
  const _GoldenParseBackend();

  @override
  SovereignMarkdownParserCapabilities get capabilities =>
      SovereignMarkdownParserCapabilities(
        parserName: 'golden-fixture',
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        supportedProfiles: const [SovereignMarkdownProfile.commonMarkGfm],
      );

  @override
  Future<SovereignMarkdownParseResult> parse(
    SovereignMarkdownParseRequest request,
  ) async {
    return _FixtureParser(request.markdown, request.revision).parse();
  }
}

final class _FixtureParser {
  _FixtureParser(this.source, this.revision);

  final String source;
  final int revision;
  final List<SovereignMarkdownBlockNode> _blocks = [];
  final List<SovereignMarkdownInlineToken> _inlineTokens = [];
  final List<SovereignMarkdownHiddenRange> _hiddenRanges = [];

  SovereignMarkdownParseResult parse() {
    _scanBlocks();
    _scanInlineTokens();
    _hiddenRanges.sort(
      (a, b) => a.sourceRange.start.compareTo(b.sourceRange.start),
    );
    return SovereignMarkdownParseResult(
      schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
      revision: revision,
      sourceTextLength: source.length,
      blocks: _blocks,
      inlineTokens: _inlineTokens,
      hiddenRanges: _hiddenRanges,
    );
  }

  void _scanBlocks() {
    var offset = 0;
    while (offset < source.length) {
      final lineEnd = _lineEnd(offset);
      final line = source.substring(offset, lineEnd);
      if (line.trim().isEmpty) {
        offset = _lineAfter(lineEnd);
        continue;
      }

      if (line.startsWith('```')) {
        offset = _scanCodeBlock(offset, lineEnd, line);
        continue;
      }
      if (line.startsWith('# ')) {
        _blocks.add(
          _block(
            SovereignMarkdownBlockKind.heading,
            'heading',
            SovereignSourceRange(offset, lineEnd),
            attributes: const {'level': 1},
          ),
        );
        _hide(offset, offset + 2, SovereignMarkdownHiddenRangeKind.blockMarker);
        offset = _lineAfter(lineEnd);
        continue;
      }
      if (line.startsWith('>')) {
        offset = _scanBlockquote(offset);
        continue;
      }
      if (_isTableStart(offset, line)) {
        offset = _scanTable(offset);
        continue;
      }

      final listMarkerLength = _listMarkerLength(line);
      if (listMarkerLength != null) {
        final checked = _taskChecked(line);
        _blocks.add(
          _block(
            SovereignMarkdownBlockKind.listItem,
            'listItem',
            SovereignSourceRange(offset, lineEnd),
            attributes: checked == null ? const {} : {'checked': checked},
          ),
        );
        _hide(
          offset,
          offset + listMarkerLength,
          SovereignMarkdownHiddenRangeKind.blockMarker,
        );
        offset = _lineAfter(lineEnd);
        continue;
      }

      offset = _scanParagraph(offset);
    }
  }

  int _scanCodeBlock(int start, int openLineEnd, String openLine) {
    var cursor = _lineAfter(openLineEnd);
    var closeStart = source.length;
    var closeEnd = source.length;
    while (cursor < source.length) {
      final lineEnd = _lineEnd(cursor);
      final line = source.substring(cursor, lineEnd);
      if (line.startsWith('```')) {
        closeStart = cursor;
        closeEnd = lineEnd;
        break;
      }
      cursor = _lineAfter(lineEnd);
    }

    final language = openLine.substring(3).trim();
    _blocks.add(
      _block(
        SovereignMarkdownBlockKind.codeBlock,
        'codeBlock',
        SovereignSourceRange(start, closeEnd),
        attributes: language.isEmpty ? const {} : {'language': language},
      ),
    );
    _hide(
      start,
      _lineAfter(openLineEnd),
      SovereignMarkdownHiddenRangeKind.markdownMarker,
    );
    if (closeStart < source.length) {
      _hide(
        closeStart > start ? closeStart - 1 : closeStart,
        closeEnd,
        SovereignMarkdownHiddenRangeKind.markdownMarker,
      );
      return _lineAfter(closeEnd);
    }
    return source.length;
  }

  int _scanBlockquote(int start) {
    var cursor = start;
    var blockEnd = start;
    while (cursor < source.length) {
      final lineEnd = _lineEnd(cursor);
      final line = source.substring(cursor, lineEnd);
      if (!line.startsWith('>')) break;
      final markerLength = _quoteMarkerLength(line);
      _hide(
        cursor,
        cursor + markerLength,
        SovereignMarkdownHiddenRangeKind.blockMarker,
      );
      blockEnd = lineEnd;
      cursor = _lineAfter(lineEnd);
    }
    _blocks.add(
      _block(
        SovereignMarkdownBlockKind.blockquote,
        'blockquote',
        SovereignSourceRange(start, blockEnd),
      ),
    );
    return cursor;
  }

  int _scanTable(int start) {
    final separatorStart = _lineAfter(_lineEnd(start));
    final separatorLine = source.substring(
      separatorStart,
      _lineEnd(separatorStart),
    );
    final alignments = _tableCells(separatorLine).map(_tableAlignment).toList();
    var cursor = start;
    var blockEnd = start;
    var rowIndex = 0;
    final rows = <SovereignMarkdownBlockNode>[];
    while (cursor < source.length) {
      final lineEnd = _lineEnd(cursor);
      final line = source.substring(cursor, lineEnd);
      if (line.trim().isEmpty || !line.contains('|')) break;
      if (rowIndex != 1) {
        rows.add(
          _block(
            SovereignMarkdownBlockKind.tableRow,
            'tableRow',
            SovereignSourceRange(cursor, lineEnd),
            attributes: {'header': rowIndex == 0},
            children: _tableCellBlocks(cursor, line),
          ),
        );
      }
      blockEnd = lineEnd;
      cursor = _lineAfter(lineEnd);
      rowIndex++;
    }

    _blocks.add(
      _block(
        SovereignMarkdownBlockKind.table,
        'table',
        SovereignSourceRange(start, blockEnd),
        attributes: {'alignments': alignments},
        children: rows,
      ),
    );
    return cursor;
  }

  int _scanParagraph(int start) {
    var cursor = start;
    var blockEnd = start;
    while (cursor < source.length) {
      final lineEnd = _lineEnd(cursor);
      final line = source.substring(cursor, lineEnd);
      if (line.trim().isEmpty || _startsSpecialBlock(cursor, line)) break;
      blockEnd = lineEnd;
      cursor = _lineAfter(lineEnd);
    }
    _blocks.add(
      _block(
        SovereignMarkdownBlockKind.paragraph,
        'paragraph',
        SovereignSourceRange(start, blockEnd),
      ),
    );
    return cursor;
  }

  bool _startsSpecialBlock(int offset, String line) {
    return line.startsWith('```') ||
        line.startsWith('# ') ||
        line.startsWith('>') ||
        _listMarkerLength(line) != null ||
        _isTableStart(offset, line);
  }

  bool _isTableStart(int offset, String line) {
    if (!line.contains('|')) return false;
    final nextStart = _lineAfter(_lineEnd(offset));
    if (nextStart >= source.length) return false;
    final nextLine = source.substring(nextStart, _lineEnd(nextStart));
    return nextLine.contains('|') && nextLine.contains('---');
  }

  void _scanInlineTokens() {
    _scanDelimited('**', SovereignMarkdownInlineKind.strong, 'strong');
    _scanDelimited(
      '~~',
      SovereignMarkdownInlineKind.strikethrough,
      'strikethrough',
    );
    _scanInlineCode();
    _scanEmphasis();
    _scanLinksAndImages();
  }

  void _scanDelimited(
    String marker,
    SovereignMarkdownInlineKind kind,
    String type,
  ) {
    var cursor = 0;
    while (cursor < source.length) {
      final start = source.indexOf(marker, cursor);
      if (start < 0) return;
      final end = source.indexOf(marker, start + marker.length);
      if (end < 0) return;
      _inlineTokens.add(
        _inline(kind, type, SovereignSourceRange(start, end + marker.length)),
      );
      _hide(
        start,
        start + marker.length,
        SovereignMarkdownHiddenRangeKind.inlineMarker,
      );
      _hide(
        end,
        end + marker.length,
        SovereignMarkdownHiddenRangeKind.inlineMarker,
      );
      cursor = end + marker.length;
    }
  }

  void _scanInlineCode() {
    var cursor = 0;
    while (cursor < source.length) {
      final start = _nextSingleBacktick(cursor);
      if (start < 0) return;
      final end = _nextSingleBacktick(start + 1);
      if (end < 0) return;
      _inlineTokens.add(
        _inline(
          SovereignMarkdownInlineKind.inlineCode,
          'inlineCode',
          SovereignSourceRange(start, end + 1),
        ),
      );
      _hide(start, start + 1, SovereignMarkdownHiddenRangeKind.inlineMarker);
      _hide(end, end + 1, SovereignMarkdownHiddenRangeKind.inlineMarker);
      cursor = end + 1;
    }
  }

  void _scanEmphasis() {
    var cursor = 0;
    while (cursor < source.length) {
      final start = _nextSingleAsterisk(cursor);
      if (start < 0) return;
      final end = _nextSingleAsterisk(start + 1);
      if (end < 0) return;
      _inlineTokens.add(
        _inline(
          SovereignMarkdownInlineKind.emphasis,
          'emphasis',
          SovereignSourceRange(start, end + 1),
        ),
      );
      _hide(start, start + 1, SovereignMarkdownHiddenRangeKind.inlineMarker);
      _hide(end, end + 1, SovereignMarkdownHiddenRangeKind.inlineMarker);
      cursor = end + 1;
    }
  }

  void _scanLinksAndImages() {
    var cursor = 0;
    while (cursor < source.length) {
      final imageStart = source.indexOf('![', cursor);
      final linkStart = source.indexOf('[', cursor);
      if (imageStart < 0 && linkStart < 0) return;

      final start =
          imageStart >= 0 && (linkStart < 0 || imageStart <= linkStart - 1)
          ? imageStart
          : linkStart;
      final isImage = start == imageStart;
      if (!isImage && start > 0 && source[start - 1] == '!') {
        cursor = start + 1;
        continue;
      }

      final labelStart = start + (isImage ? 2 : 1);
      final labelEnd = source.indexOf(']', labelStart);
      if (labelEnd < 0 || labelEnd + 1 >= source.length) return;
      if (source[labelEnd + 1] != '(') {
        cursor = labelEnd + 1;
        continue;
      }
      final destinationEnd = source.indexOf(')', labelEnd + 2);
      if (destinationEnd < 0) return;

      final tokenEnd = destinationEnd + 1;
      final label = source.substring(labelStart, labelEnd);
      final destination = source.substring(labelEnd + 2, destinationEnd);
      _inlineTokens.add(
        _inline(
          isImage
              ? SovereignMarkdownInlineKind.image
              : SovereignMarkdownInlineKind.link,
          isImage ? 'image' : 'link',
          SovereignSourceRange(start, tokenEnd),
          attributes: isImage
              ? {'src': destination, 'alt': label}
              : {'destination': destination, 'label': label},
        ),
      );
      _hide(
        start,
        start + (isImage ? 2 : 1),
        SovereignMarkdownHiddenRangeKind.inlineMarker,
      );
      _hide(
        labelEnd,
        tokenEnd,
        SovereignMarkdownHiddenRangeKind.linkDestination,
      );
      cursor = tokenEnd;
    }
  }

  int _nextSingleAsterisk(int start) {
    var cursor = start;
    while (cursor < source.length) {
      final index = source.indexOf('*', cursor);
      if (index < 0) return -1;
      if (!_isEscaped(index) &&
          (index == 0 || source[index - 1] != '*') &&
          (index + 1 >= source.length || source[index + 1] != '*')) {
        return index;
      }
      cursor = index + 1;
    }
    return -1;
  }

  int _nextSingleBacktick(int start) {
    var cursor = start;
    while (cursor < source.length) {
      final index = source.indexOf('`', cursor);
      if (index < 0) return -1;
      if (!_isEscaped(index) &&
          (index == 0 || source[index - 1] != '`') &&
          (index + 1 >= source.length || source[index + 1] != '`')) {
        return index;
      }
      cursor = index + 1;
    }
    return -1;
  }

  bool _isEscaped(int index) {
    return index > 0 && source[index - 1] == r'\';
  }

  int _lineEnd(int start) {
    final end = source.indexOf('\n', start);
    return end < 0 ? source.length : end;
  }

  int _lineAfter(int lineEnd) {
    return lineEnd < source.length ? lineEnd + 1 : lineEnd;
  }

  SovereignMarkdownBlockNode _block(
    SovereignMarkdownBlockKind kind,
    String type,
    SovereignSourceRange range, {
    Map<String, Object?> attributes = const {},
    Iterable<SovereignMarkdownBlockNode> children = const [],
  }) {
    return SovereignMarkdownBlockNode(
      kind: kind,
      type: type,
      sourceRange: range,
      attributes: attributes,
      children: children,
    );
  }

  SovereignMarkdownInlineToken _inline(
    SovereignMarkdownInlineKind kind,
    String type,
    SovereignSourceRange range, {
    Map<String, Object?> attributes = const {},
  }) {
    return SovereignMarkdownInlineToken(
      kind: kind,
      type: type,
      sourceRange: range,
      attributes: attributes,
    );
  }

  void _hide(int start, int end, SovereignMarkdownHiddenRangeKind kind) {
    if (start >= end) return;
    _hiddenRanges.add(
      SovereignMarkdownHiddenRange(
        kind: kind,
        type: switch (kind) {
          SovereignMarkdownHiddenRangeKind.blockMarker => 'blockMarker',
          SovereignMarkdownHiddenRangeKind.inlineMarker => 'inlineMarker',
          SovereignMarkdownHiddenRangeKind.linkDestination => 'linkDestination',
          SovereignMarkdownHiddenRangeKind.linkTitle => 'linkTitle',
          SovereignMarkdownHiddenRangeKind.markdownMarker => 'markdownMarker',
          SovereignMarkdownHiddenRangeKind.referenceDefinition =>
            'referenceDefinition',
          SovereignMarkdownHiddenRangeKind.rawHtml => 'rawHtml',
          SovereignMarkdownHiddenRangeKind.escapeMarker => 'escapeMarker',
          SovereignMarkdownHiddenRangeKind.unknown => 'unknown',
        },
        sourceRange: SovereignSourceRange(start, end),
      ),
    );
  }
}

int? _listMarkerLength(String line) {
  if (line.startsWith('- [x] ') ||
      line.startsWith('- [X] ') ||
      line.startsWith('- [ ] ')) {
    return 6;
  }
  if (line.startsWith('- ')) return 2;
  return null;
}

bool? _taskChecked(String line) {
  if (line.startsWith('- [x] ') || line.startsWith('- [X] ')) return true;
  if (line.startsWith('- [ ] ')) return false;
  return null;
}

int _quoteMarkerLength(String line) {
  var cursor = 0;
  while (cursor < line.length && line[cursor] == '>') {
    cursor++;
    if (cursor < line.length && line[cursor] == ' ') cursor++;
  }
  return cursor;
}

List<String> _tableCells(String line) {
  var normalized = line.trim();
  if (normalized.startsWith('|')) normalized = normalized.substring(1);
  if (normalized.endsWith('|')) {
    normalized = normalized.substring(0, normalized.length - 1);
  }
  return normalized.split('|').map((cell) => cell.trim()).toList();
}

List<SovereignMarkdownBlockNode> _tableCellBlocks(int lineStart, String line) {
  var start = 0;
  var end = line.length;
  if (line.startsWith('|')) start = 1;
  if (end > start && line.endsWith('|')) end--;

  final cells = <SovereignMarkdownBlockNode>[];
  var cellStart = start;
  for (var index = start; index < end; index++) {
    if (line.codeUnitAt(index) != 124) continue;
    cells.add(_tableCellBlock(lineStart, line, cellStart, index));
    cellStart = index + 1;
  }
  cells.add(_tableCellBlock(lineStart, line, cellStart, end));
  return cells;
}

SovereignMarkdownBlockNode _tableCellBlock(
  int lineStart,
  String line,
  int start,
  int end,
) {
  var contentStart = start;
  var contentEnd = end;
  while (contentStart < contentEnd &&
      (line.codeUnitAt(contentStart) == 32 ||
          line.codeUnitAt(contentStart) == 9)) {
    contentStart++;
  }
  while (contentEnd > contentStart &&
      (line.codeUnitAt(contentEnd - 1) == 32 ||
          line.codeUnitAt(contentEnd - 1) == 9)) {
    contentEnd--;
  }
  return SovereignMarkdownBlockNode(
    kind: SovereignMarkdownBlockKind.tableCell,
    type: 'tableCell',
    sourceRange: SovereignSourceRange(
      lineStart + contentStart,
      lineStart + contentEnd,
    ),
  );
}

String _tableAlignment(String cell) {
  final trimmed = cell.trim();
  final left = trimmed.startsWith(':');
  final right = trimmed.endsWith(':');
  if (left && right) return 'center';
  if (left) return 'left';
  if (right) return 'right';
  return 'none';
}

Future<void> _loadGoldenFonts() async {
  final flutterRoot = _flutterRoot();
  if (flutterRoot == null) return;

  final fontDirectory = Directory(
    '$flutterRoot/bin/cache/artifacts/material_fonts',
  );
  if (!fontDirectory.existsSync()) return;

  await _loadFontFamily('Roboto', fontDirectory, const [
    'Roboto-Regular.ttf',
    'Roboto-Bold.ttf',
    'Roboto-Italic.ttf',
  ]);
  await _loadFontFamily('monospace', fontDirectory, const [
    'Roboto-Regular.ttf',
  ]);
}

Future<void> _loadFontFamily(
  String family,
  Directory fontDirectory,
  List<String> fileNames,
) async {
  final loader = FontLoader(family);
  var hasFont = false;
  for (final fileName in fileNames) {
    final file = File('${fontDirectory.path}/$fileName');
    if (!file.existsSync()) continue;
    final bytes = await file.readAsBytes();
    loader.addFont(Future.value(ByteData.sublistView(bytes)));
    hasFont = true;
  }
  if (hasFont) await loader.load();
}

String? _flutterRoot() {
  final fromEnvironment = Platform.environment['FLUTTER_ROOT'];
  if (fromEnvironment != null && fromEnvironment.isNotEmpty) {
    return fromEnvironment;
  }

  final executable = Platform.resolvedExecutable;
  const dartSdkMarker = '/bin/cache/dart-sdk/bin/dart';
  final markerIndex = executable.indexOf(dartSdkMarker);
  if (markerIndex <= 0) return null;
  return executable.substring(0, markerIndex);
}
