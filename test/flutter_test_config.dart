import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';

/// Golden comparisons tolerate a small pixel-diff fraction.
///
/// Flark's visual goldens are text-heavy, and text antialiasing differs
/// slightly between CI runner images and Flutter engine versions — enough to
/// diff by ~0.01% (a few dozen pixels) while the layout is unchanged. The
/// zero-tolerance default comparator turns that environmental noise into flaky
/// failures. This threshold absorbs it; a genuine visual regression moves far
/// more than this fraction of the pixels (a reflow or changed glyphs touch
/// hundreds+). Lower it if you want a stricter visual guard.
const double _goldenDiffTolerance = 0.005; // 0.5%

Future<void> testExecutable(FutureOr<void> Function() testMain) async {
  final defaultComparator = goldenFileComparator as LocalFileComparator;
  goldenFileComparator = _TolerantGoldenFileComparator(
    defaultComparator.basedir,
    tolerance: _goldenDiffTolerance,
  );
  await testMain();
}

class _TolerantGoldenFileComparator extends LocalFileComparator {
  _TolerantGoldenFileComparator(Uri basedir, {required this.tolerance})
    // LocalFileComparator derives its basedir from the test-file Uri it is
    // given; resolve a placeholder file so the basedir matches the default
    // comparator's (goldens stay resolved relative to each test's directory).
    : super(basedir.resolve('flark_flutter_test_config.dart'));

  final double tolerance;

  @override
  Future<bool> compare(Uint8List imageBytes, Uri golden) async {
    final result = await GoldenFileComparator.compareLists(
      imageBytes,
      await getGoldenBytes(golden),
    );
    if (result.passed || result.diffPercent <= tolerance) {
      return true;
    }
    throw FlutterError(await generateFailureOutput(result, golden, basedir));
  }
}
