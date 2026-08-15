import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:flark_flutter/flark_flutter_advanced.dart';

import '../support/flark_test_paths.dart';

/// RFC 022 §4: the adoption-time confirmation runs in debug builds, so this
/// suite drives the highest-stakes authored-claim flow (an armed wrap, which
/// writes delimiters and pre-hides them before any parse has seen them)
/// against the real comrak bridge and proves the whole loop closes: the claim
/// is confirmed, nothing throws, and the geometry telemetry stays quiet.
void main() {
  final libPath = flarkNativeBridgeLibraryPathForPlatform();
  if (libPath.isEmpty || !File(libPath).existsSync()) {
    test('native bridge not built; adoption confirmation suite skipped', () {
      expect(true, isTrue);
    });
    return;
  }

  test(
    'an armed wrap adopts with its authored claim parser-confirmed',
    () async {
      expect(
        flarkDebugValidatePredictionAdoption,
        isTrue,
        reason: 'the RFC 022 confirmation must be on by default',
      );
      final controller = FlarkFlutterController.fromMarkdown(
        '',
        extensions: FlarkMarkdownEditingExtensions.standard(),
        parseBackend: FlarkNativeComrakParseBackend.withNativeBridge(
          overrideLibraryPath: libPath,
        ),
      );
      addTearDown(controller.dispose);

      final unconfirmedBefore = flarkDebugUnconfirmedPredictionRanges;
      controller.togglePendingInlineStyle(FlarkMarkdownInlineStyle.strong);
      expect(
        controller.applyProjectedTextEdit(
          oldDisplayText: '',
          newDisplayText: 'h',
          newDisplayCaret: 1,
        ),
        isTrue,
      );
      // The armed wrap authored `**h**` and pre-hid both markers; the parse now
      // re-derives them. If the placement had written markdown comrak disagrees
      // with, the RFC 022 assert inside adoption would throw here.
      expect(controller.markdown, '**h**');
      await controller.parseNow();
      expect(controller.hasAuthoritativeRenderPlan, isTrue);
      expect(
        flarkDebugUnconfirmedPredictionRanges,
        unconfirmedBefore,
        reason:
            'the canonical armed-wrap flow must confirm all predicted '
            'ranges — growth here means geometry prediction went stale on the '
            'happy path',
      );
    },
  );
}
