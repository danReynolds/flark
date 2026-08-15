import 'dart:io';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  setUpAll(() async {
    await Future.wait([
      _loadFont('Phase0Arabic', '/System/Library/Fonts/GeezaPro.ttc'),
      _loadFont(
        'Phase0Devanagari',
        '/System/Library/Fonts/Supplemental/DevanagariMT.ttc',
      ),
      _loadFont(
        'Phase0Thai',
        '/System/Library/Fonts/Supplemental/Thonburi.ttc',
      ),
      _loadFont(
        'Phase0Latin',
        '/System/Library/Fonts/Supplemental/Times New Roman.ttf',
      ),
    ]);
  });

  test('arbitrary complex-script shard seams change shaping', () {
    final cases = <_ShapingCase>[
      const _ShapingCase(
        label: 'arabic_join',
        left: 'سل',
        right: 'ام',
        style: TextStyle(fontFamily: 'Phase0Arabic', fontSize: 42),
        direction: TextDirection.rtl,
      ),
      const _ShapingCase(
        label: 'devanagari_cluster',
        left: 'क',
        right: 'र्म',
        style: TextStyle(fontFamily: 'Phase0Devanagari', fontSize: 42),
      ),
      const _ShapingCase(
        label: 'thai_word',
        left: 'ภา',
        right: 'ษาไทย',
        style: TextStyle(fontFamily: 'Phase0Thai', fontSize: 42),
      ),
      const _ShapingCase(
        label: 'latin_ligature',
        left: 'of',
        right: 'fice',
        style: TextStyle(
          fontFamily: 'Phase0Latin',
          fontSize: 42,
          fontFeatures: [FontFeature.enable('liga')],
        ),
      ),
    ];

    final deltas = <String, double>{};
    for (final sample in cases) {
      final whole = _unwrappedWidth(
        sample.left + sample.right,
        sample.style,
        sample.direction,
      );
      final split =
          _unwrappedWidth(sample.left, sample.style, sample.direction) +
          _unwrappedWidth(sample.right, sample.style, sample.direction);
      deltas[sample.label] = (whole - split).abs();
    }

    final materialMismatches = deltas.values
        .where((delta) => delta > 0.1)
        .length;
    expect(
      materialMismatches,
      greaterThanOrEqualTo(2),
      reason:
          'bounded layout shards cannot split arbitrary source or even '
          'arbitrary grapheme boundaries without shaping context',
    );

    const arabicStyle = TextStyle(fontFamily: 'Phase0Arabic', fontSize: 42);
    final safeWhole = _unwrappedWidth(
      'سلام عليكم',
      arabicStyle,
      TextDirection.rtl,
    );
    final safeSplit =
        _unwrappedWidth('سلام ', arabicStyle, TextDirection.rtl) +
        _unwrappedWidth('عليكم', arabicStyle, TextDirection.rtl);
    final safeDelta = (safeWhole - safeSplit).abs();
    expect(safeDelta, lessThan(0.1));

    // ignore: avoid_print
    print(
      'flark_phase0_native_shaping unsafe_deltas=$deltas '
      'material_mismatches=$materialMismatches '
      'whitespace_seam_delta=$safeDelta',
    );
  });

  test('independent wrapped shards do not preserve paragraph line state', () {
    const style = TextStyle(fontFamily: 'Phase0Arabic', fontSize: 28);
    final text = List<String>.filled(
      500,
      'سلام عليكم كتابة عربية طويلة للاختبار ',
    ).join();
    const width = 320.0;
    final monolithicLines = _wrappedLineCount(
      text,
      style,
      width,
      TextDirection.rtl,
    );
    var mismatches = 0;
    var samples = 0;
    for (var target = 300; target < text.length - 300; target += 211) {
      final splitAt = text.indexOf(' ', target);
      if (splitAt < 0) break;
      final independentLines =
          _wrappedLineCount(
            text.substring(0, splitAt + 1),
            style,
            width,
            TextDirection.rtl,
          ) +
          _wrappedLineCount(
            text.substring(splitAt + 1),
            style,
            width,
            TextDirection.rtl,
          );
      samples += 1;
      if (independentLines != monolithicLines) mismatches += 1;
    }
    expect(mismatches, greaterThan(0));

    final longComplex = List<String>.filled(
      4096,
      'سلامعليكمكتابةعربيةطويلة ',
    ).join();
    final stopwatch = Stopwatch()..start();
    final lines = _wrappedLineCount(
      longComplex,
      style,
      width,
      TextDirection.rtl,
    );
    stopwatch.stop();

    // ignore: avoid_print
    print(
      'flark_phase0_native_wrap source_units=${longComplex.length} '
      'lines=$lines full_layout_us=${stopwatch.elapsedMicroseconds} '
      'split_samples=$samples propagation_mismatches=$mismatches',
    );
  });

  test('bounded pre/post shaping context repairs joining seams', () {
    final cases = <_ContextCase>[
      const _ContextCase(
        label: 'arabic_continuous',
        text: 'سلامسلامسلامسلامسلامسلامسلامسلامسلامسلام',
        seam: 20,
        context: 8,
        style: TextStyle(fontFamily: 'Phase0Arabic', fontSize: 42),
        direction: TextDirection.rtl,
      ),
      const _ContextCase(
        label: 'latin_ligature_run',
        text: 'officeofficeofficeofficeofficeoffice',
        seam: 15,
        context: 8,
        style: TextStyle(
          fontFamily: 'Phase0Latin',
          fontSize: 42,
          fontFeatures: [FontFeature.enable('liga')],
        ),
      ),
    ];

    final noContextDeltas = <String, double>{};
    final contextDeltas = <String, double>{};
    for (final sample in cases) {
      final leftStart = math.max(0, sample.seam - sample.context);
      final rightEnd = math.min(
        sample.text.length,
        sample.seam + sample.context,
      );
      final whole = _selectedWidth(
        sample.text,
        sample.style,
        sample.direction,
        leftStart,
        rightEnd,
      );
      final leftWithoutContext = sample.text.substring(leftStart, sample.seam);
      final rightWithoutContext = sample.text.substring(sample.seam, rightEnd);
      final noContext =
          _selectedWidth(
            leftWithoutContext,
            sample.style,
            sample.direction,
            0,
            leftWithoutContext.length,
          ) +
          _selectedWidth(
            rightWithoutContext,
            sample.style,
            sample.direction,
            0,
            rightWithoutContext.length,
          );

      final window = sample.text.substring(leftStart, rightEnd);
      final seamInWindow = sample.seam - leftStart;
      final withContext =
          _selectedWidth(
            window,
            sample.style,
            sample.direction,
            0,
            seamInWindow,
          ) +
          _selectedWidth(
            window,
            sample.style,
            sample.direction,
            seamInWindow,
            rightEnd - leftStart,
          );

      noContextDeltas[sample.label] = (whole - noContext).abs();
      contextDeltas[sample.label] = (whole - withContext).abs();
    }

    expect(
      contextDeltas['arabic_continuous']!,
      lessThan(noContextDeltas['arabic_continuous']!),
    );
    expect(
      contextDeltas['latin_ligature_run']!,
      lessThanOrEqualTo(noContextDeltas['latin_ligature_run']!),
    );

    // ignore: avoid_print
    print(
      'flark_phase0_shaping_context no_context=$noContextDeltas '
      'context=$contextDeltas context_units=8',
    );
  });
}

Future<void> _loadFont(String family, String path) async {
  final bytes = await File(path).readAsBytes();
  final loader = FontLoader(family)
    ..addFont(Future<ByteData>.value(ByteData.sublistView(bytes)));
  await loader.load();
}

double _unwrappedWidth(String text, TextStyle style, TextDirection direction) {
  final painter = TextPainter(
    text: TextSpan(text: text, style: style),
    textDirection: direction,
  )..layout();
  final width = painter.width;
  painter.dispose();
  return width;
}

int _wrappedLineCount(
  String text,
  TextStyle style,
  double width,
  TextDirection direction,
) {
  final painter = TextPainter(
    text: TextSpan(text: text, style: style),
    textDirection: direction,
  )..layout(maxWidth: width);
  final count = math.max(1, painter.computeLineMetrics().length);
  painter.dispose();
  return count;
}

double _selectedWidth(
  String text,
  TextStyle style,
  TextDirection direction,
  int start,
  int end,
) {
  if (text.isEmpty || start == end) return 0;
  final painter = TextPainter(
    text: TextSpan(text: text, style: style),
    textDirection: direction,
  )..layout();
  final width = painter
      .getBoxesForSelection(TextSelection(baseOffset: start, extentOffset: end))
      .fold<double>(0, (sum, box) => sum + box.toRect().width);
  painter.dispose();
  return width;
}

final class _ShapingCase {
  const _ShapingCase({
    required this.label,
    required this.left,
    required this.right,
    required this.style,
    this.direction = TextDirection.ltr,
  });

  final String label;
  final String left;
  final String right;
  final TextStyle style;
  final TextDirection direction;
}

final class _ContextCase {
  const _ContextCase({
    required this.label,
    required this.text,
    required this.seam,
    required this.context,
    required this.style,
    this.direction = TextDirection.ltr,
  });

  final String label;
  final String text;
  final int seam;
  final int context;
  final TextStyle style;
  final TextDirection direction;
}
