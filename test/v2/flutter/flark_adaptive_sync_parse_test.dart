import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:flark/flark_advanced.dart';

import '../support/flark_test_paths.dart';

/// RFC 022 §6: the sync-parse ceiling is latency-learned, so documents far
/// past the old fixed 4 KiB isolate threshold still parse authoritatively in
/// the same event turn on a machine that affords it.
void main() {
  final libPath = flarkNativeBridgeLibraryPathForPlatform();
  if (libPath.isEmpty || !File(libPath).existsSync()) {
    test('native bridge not built; adaptive sync parse suite skipped', () {
      expect(true, isTrue);
    });
    return;
  }

  test('a mid-size document parses synchronously past the fixed threshold',
      () {
    // ~40 KiB — an order of magnitude past the old 4 KiB sync cutoff, and
    // structured (headings, emphasis) so the parse is not trivially empty.
    final markdown = List.generate(
      800,
      (index) => '## Section $index\n\nSome **bold** and *emphasis* text.\n',
    ).join('\n');
    expect(markdown.length, greaterThan(40000));
    expect(markdown.length, lessThan(60000),
        reason: 'stay under the adaptive ceiling’s 64 KiB starting value '
            'so the first sync attempt is affordable');

    final controller = FlarkFlutterController.fromMarkdown(
      markdown,
      extensions: FlarkMarkdownEditingExtensions.standard(),
      parseBackend: FlarkNativeComrakParseBackend.withNativeBridge(
        overrideLibraryPath: libPath,
      ),
    );
    addTearDown(controller.dispose);

    expect(
      controller.tryParseSync(),
      isTrue,
      reason: 'the adaptive ceiling must afford a 40 KiB document without '
          'the worker isolate (old fixed threshold: 4 KiB)',
    );
    expect(controller.hasAuthoritativeRenderPlan, isTrue);
  });

  test('the keystroke path keeps its debounce (device-gated; RFC 022 §6)',
      () async {
    // Adopting a parse between two keystrokes would shrink the pre-parse
    // windows the live-edit echo recognizers assume, and that behavior class
    // is gated on the manual IME device pass. Until Phase 4 lands under that
    // gate, an ordinary edit must NOT become authoritative from microtasks
    // alone — only from the debounced parse (or an explicit parseNow).
    final controller = FlarkFlutterController.fromMarkdown(
      '# Title\n\nSome **bold** text.',
      extensions: FlarkMarkdownEditingExtensions.standard(),
      parseBackend: FlarkNativeComrakParseBackend.withNativeBridge(
        overrideLibraryPath: libPath,
      ),
    );
    addTearDown(controller.dispose);
    expect(controller.tryParseSync(), isTrue);
    controller.ensureParsing();

    final result = controller.dispatch(
      command: FlarkCoreEditingCommands.insertText,
      payload: const FlarkInsertTextPayload('x'),
    );
    expect(result.commandResult.isHandled, isTrue);
    await Future<void>.value();
    await Future<void>.value();
    expect(
      controller.hasAuthoritativeRenderPlan,
      isFalse,
      reason: 'a plain insertText declares no authored markers, so it must '
          'wait for the debounced parse — same-turn adoption on the typing '
          'path is deferred to the device-gated Phase 4',
    );
    await controller.parseNow();
    expect(controller.hasAuthoritativeRenderPlan, isTrue);
  });
}
