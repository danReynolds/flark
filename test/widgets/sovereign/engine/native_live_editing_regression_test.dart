import 'dart:io';
import 'dart:math';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/commonmark_syntax_engine_adapter.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/native_comrak_parse_backend.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_engine.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_snapshot.dart';
import 'support/test_paths.dart';

void main() {
  group('Native live editing regressions', () {
    final libPath = sovereignNativeBridgeLibraryPathForPlatform();

    if (libPath.isEmpty || !File(libPath).existsSync()) {
      test('native bridge not built; native live editing suite skipped', () {
        expect(true, isTrue);
      });
      return;
    }

    test('fenced code enter/arrow exits remain stable with native backend', () {
      final controller = _nativeController(
        libPath: libPath,
        text: '```\ncode\n\n```\nnext',
      );
      addTearDown(controller.dispose);

      final codeStart = controller.text.indexOf('code');
      expect(codeStart, isNot(-1));

      controller.selection = TextSelection.collapsed(
        offset: codeStart + 'code'.length,
      );
      expect(controller.handleArrowDownKey(), isTrue);
      expect(controller.selection.baseOffset, controller.text.indexOf('next'));

      controller.selection = TextSelection.collapsed(offset: codeStart + 1);
      expect(controller.handleArrowUpKey(), isTrue);
      expect(controller.selection.baseOffset, 0);

      final trimController = _nativeController(
        libPath: libPath,
        text: '```\ncode\n\n```\nnext',
      );
      addTearDown(trimController.dispose);

      final blankLineMarker = trimController.text.indexOf('\n\n```');
      expect(blankLineMarker, isNot(-1));
      trimController.selection = TextSelection.collapsed(
        offset: blankLineMarker + 1,
      );
      trimController.handleEnter();

      expect(trimController.text, '```\ncode\n```\nnext');
      expect(trimController.selection.baseOffset, '```\ncode\n```\n'.length);
    });

    test(
      'blockquote continuation and exits remain stable with native backend',
      () {
        final controller = _nativeController(libPath: libPath, text: '> alpha');
        addTearDown(controller.dispose);

        controller.selection = TextSelection.collapsed(
          offset: controller.text.length,
        );
        controller.handleEnter();
        expect(controller.text, '> alpha\n> ');

        controller.handleEnter();
        expect(controller.text, '> alpha\n\n');

        final exitController = _nativeController(
          libPath: libPath,
          text: '> alpha\n> \nnext',
        );
        addTearDown(exitController.dispose);

        exitController.selection = const TextSelection.collapsed(offset: 2);
        expect(exitController.handleArrowDownKey(), isTrue);
        expect(
          exitController.selection.baseOffset,
          equals('> alpha\n> \n'.length + 2),
        );
      },
    );

    test(
      'list continuation/exit/backspace boundary remain stable with native backend',
      () {
        final continueController = _nativeController(
          libPath: libPath,
          text: '- item',
        );
        addTearDown(continueController.dispose);
        final continueCaret = continueController.text.length;
        continueController.selection = TextSelection.collapsed(
          offset: continueCaret,
        );
        continueController.value = TextEditingValue(
          text: continueController.text.replaceRange(
            continueCaret,
            continueCaret,
            '\n',
          ),
          selection: TextSelection.collapsed(offset: continueCaret + 1),
        );
        expect(continueController.text, '- item\n- ');

        final exitController = _nativeController(libPath: libPath, text: '- ');
        addTearDown(exitController.dispose);
        exitController.selection = const TextSelection.collapsed(offset: 2);
        exitController.value = const TextEditingValue(
          text: '- \n',
          selection: TextSelection.collapsed(offset: 3),
        );
        expect(exitController.text, '\n');

        final backspaceController = _nativeController(
          libPath: libPath,
          text: '- item',
        );
        addTearDown(backspaceController.dispose);
        backspaceController.selection = const TextSelection.collapsed(
          offset: 2,
        );
        backspaceController.value = const TextEditingValue(
          text: '-item',
          selection: TextSelection.collapsed(offset: 1),
        );
        expect(backspaceController.text, 'item');
        expect(backspaceController.selection.baseOffset, 0);
      },
    );

    test(
      'inline toolbar insertion and backspace re-entry remain stable with native backend',
      () {
        final controller = _nativeController(libPath: libPath, text: '');
        addTearDown(controller.dispose);

        controller.value = const TextEditingValue(
          text: '****',
          selection: TextSelection.collapsed(offset: 2),
        );
        final hidden = controller.decoration.hiddenRanges;
        final splitMarkers =
            hidden.contains(const TextRange(start: 0, end: 2)) &&
                hidden.contains(const TextRange(start: 2, end: 4));
        final mergedMarkers = hidden.contains(
          const TextRange(start: 0, end: 4),
        );
        final fullyCovered = () {
          for (var offset = 0; offset < 4; offset++) {
            final covered = hidden.any(
              (r) => offset >= r.start && offset < r.end,
            );
            if (!covered) return false;
          }
          return true;
        }();
        expect(
          splitMarkers || mergedMarkers || fullyCovered,
          isTrue,
          reason:
              'Adjacent hidden inline markers may be split/merged/normalized '
              'as long as all wrapper marker bytes stay hidden.',
        );

        controller.value = const TextEditingValue(
          text: '**x**',
          selection: TextSelection.collapsed(offset: 3),
        );
        expect(
          controller.decoration.hiddenRanges,
          containsAll(<TextRange>[
            TextRange(start: 0, end: 2),
            TextRange(start: 3, end: 5),
          ]),
        );

        const original = '**abc**';
        controller.value = const TextEditingValue(
          text: original,
          selection: TextSelection.collapsed(offset: original.length),
        );
        controller.value = _singleCharBackspaceValue(controller.value);

        expect(controller.text, '**ab**');
        expect(controller.selection, const TextSelection.collapsed(offset: 4));
        expect(
          controller.decoration.hiddenRanges,
          containsAll(<TextRange>[
            TextRange(start: 0, end: 2),
            TextRange(start: 4, end: 6),
          ]),
        );
      },
    );

    test(
      'snapshot-gap cursor safety holds with native backend during delayed parses',
      () {
        final engine = _DelayedNativeEngine(_nativeSyntaxEngine(libPath));
        final controller = SovereignController(syntaxEngine: engine);
        addTearDown(controller.dispose);

        final random = Random(13);
        const tokens = <String>[
          '**x**',
          '_x_',
          '`x`',
          '```\ncode\n```',
          '# heading',
          '> quote',
          '- item',
          'plain',
        ];

        for (var i = 0; i < 120; i++) {
          final prefix = String.fromCharCodes(
            List<int>.generate(
              random.nextInt(3),
              (_) => 97 + random.nextInt(3),
            ),
          );
          final token = tokens[random.nextInt(tokens.length)];
          final suffix = String.fromCharCodes(
            List<int>.generate(
              random.nextInt(3),
              (_) => 109 + random.nextInt(3),
            ),
          );
          final text = '$prefix$token$suffix';

          final preview = engine.previewPrediction(text);
          final requested = _requestedOffsetInsideFirstMarker(preview, text);
          controller.value = TextEditingValue(
            text: text,
            selection: TextSelection.collapsed(offset: requested),
          );

          _expectSelectionOutsideMarkerInteriors(
            controller.selection.baseOffset,
            controller.decoration.hiddenRanges,
          );
        }
      },
    );
  });
}

SovereignController _nativeController({
  required String libPath,
  required String text,
}) {
  return SovereignController(
    text: text,
    syntaxEngine: _nativeSyntaxEngine(libPath),
  );
}

CommonMarkSyntaxEngineAdapter _nativeSyntaxEngine(String libPath) {
  return CommonMarkSyntaxEngineAdapter(
    parseBackend: ComrakCommonMarkParseBackend.withNativeBridge(
      overrideLibraryPath: libPath,
    ),
  );
}

void _expectSelectionOutsideMarkerInteriors(
  int selectionOffset,
  List<TextRange> markers,
) {
  for (final range in markers) {
    if (range.end <= range.start) continue;
    final inside = selectionOffset > range.start && selectionOffset < range.end;
    expect(
      inside,
      isFalse,
      reason: 'Caret landed inside hidden marker interior at $selectionOffset '
          'for range [${range.start}, ${range.end})',
    );
  }
}

int _requestedOffsetInsideFirstMarker(SyntaxPrediction preview, String text) {
  for (final range in preview.markerRanges) {
    if (range.end - range.start > 1) {
      return (range.start + 1).clamp(0, text.length);
    }
  }
  return text.length;
}

TextEditingValue _singleCharBackspaceValue(TextEditingValue value) {
  final selection = value.selection;
  if (!selection.isCollapsed || !selection.isValid) return value;
  final caret = selection.baseOffset;
  if (caret <= 0 || caret > value.text.length) return value;
  return TextEditingValue(
    text: value.text.replaceRange(caret - 1, caret, ''),
    selection: TextSelection.collapsed(offset: caret - 1),
  );
}

class _DelayedNativeEngine implements SyntaxEngine {
  final CommonMarkSyntaxEngineAdapter _delegate;

  _DelayedNativeEngine(this._delegate);

  @override
  Future<SyntaxSnapshot> parse(SyntaxParseRequest request) async {
    await Future<void>.delayed(const Duration(milliseconds: 25));
    return _delegate.parse(request);
  }

  @override
  SyntaxPrediction predict(SyntaxPredictRequest request) {
    return _delegate.predict(request);
  }

  SyntaxPrediction previewPrediction(String text) {
    return _delegate.predict(SyntaxPredictRequest(revision: 0, text: text));
  }
}
