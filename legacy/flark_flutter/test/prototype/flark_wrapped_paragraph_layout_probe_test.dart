@Tags(<String>['benchmark'])
library;

import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('full TextPainter relayout scales with one wrapped paragraph', () {
    final receipts = <_LayoutReceipt>[];
    for (final sizeKiB in const [4, 16, 64, 256, 1024]) {
      var source = _paragraphOfSize(sizeKiB * 1024);
      final painter = TextPainter(
        text: TextSpan(text: source, style: _style),
        textDirection: TextDirection.ltr,
      );
      painter.layout(maxWidth: 560);

      final cases = switch (sizeKiB) {
        <= 16 => 21,
        <= 64 => 13,
        <= 256 => 7,
        _ => 5,
      };
      final editMicros = <int>[];
      final layoutMicros = <int>[];
      final offset = source.length ~/ 2;
      for (var iteration = 0; iteration < cases; iteration += 1) {
        final edit = Stopwatch()..start();
        source = source.replaceRange(
          offset,
          offset + 1,
          iteration.isEven ? 'x' : 'y',
        );
        edit.stop();
        editMicros.add(edit.elapsedMicroseconds);

        final layout = Stopwatch()..start();
        painter.text = TextSpan(text: source, style: _style);
        painter.layout(maxWidth: 560);
        layout.stop();
        layoutMicros.add(layout.elapsedMicroseconds);
      }
      editMicros.sort();
      layoutMicros.sort();
      receipts.add(
        _LayoutReceipt(
          sizeKiB: sizeKiB,
          lines: painter.computeLineMetrics().length,
          height: painter.height,
          editP50Micros: _percentile(editMicros, 50),
          editP95Micros: _percentile(editMicros, 95),
          layoutP50Micros: _percentile(layoutMicros, 50),
          layoutP95Micros: _percentile(layoutMicros, 95),
          layoutMaxMicros: layoutMicros.last,
        ),
      );
      painter.dispose();
    }

    for (final receipt in receipts) {
      debugPrint(
        'flark_wrapped_paragraph size_kib=${receipt.sizeKiB} '
        'visual_lines=${receipt.lines} height=${receipt.height.toStringAsFixed(1)} '
        'edit_p50_us=${receipt.editP50Micros} '
        'edit_p95_us=${receipt.editP95Micros} '
        'layout_p50_us=${receipt.layoutP50Micros} '
        'layout_p95_us=${receipt.layoutP95Micros} '
        'layout_max_us=${receipt.layoutMaxMicros}',
      );
    }

    expect(receipts.last.lines, greaterThan(receipts.first.lines));
    expect(
      receipts.last.layoutP50Micros,
      greaterThan(receipts.first.layoutP50Micros),
    );
  });
}

const _style = TextStyle(fontSize: 16, height: 1.35);

final class _LayoutReceipt {
  const _LayoutReceipt({
    required this.sizeKiB,
    required this.lines,
    required this.height,
    required this.editP50Micros,
    required this.editP95Micros,
    required this.layoutP50Micros,
    required this.layoutP95Micros,
    required this.layoutMaxMicros,
  });

  final int sizeKiB;
  final int lines;
  final double height;
  final int editP50Micros;
  final int editP95Micros;
  final int layoutP50Micros;
  final int layoutP95Micros;
  final int layoutMaxMicros;
}

String _paragraphOfSize(int size) {
  const chunk =
      'alpha beta gamma delta epsilon zeta eta theta iota kappa lambda '
      '**bold words** and `inline code` continue through the paragraph ';
  final output = StringBuffer();
  while (output.length < size) {
    output.write(chunk);
  }
  return output.toString().substring(0, size);
}

int _percentile(List<int> values, int percentile) =>
    values[((values.length - 1) * percentile) ~/ 100];
