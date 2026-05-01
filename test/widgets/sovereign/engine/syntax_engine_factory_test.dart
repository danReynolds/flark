import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/commonmark_syntax_engine_adapter.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/native_comrak_ffi.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/native_comrak_parse_backend.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_engine.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_engine_factory.dart';

void main() {
  test('factory returns CommonMark adapter', () {
    final engine = SyntaxEngineFactory.create();
    expect(engine, isA<CommonMarkSyntaxEngineAdapter>());
    final adapter = engine as CommonMarkSyntaxEngineAdapter;
    expect(adapter.parseBackend, isA<ComrakCommonMarkParseBackend>());
  });

  test('factory throws when native bridge is missing', () {
    expect(
      () => SyntaxEngineFactory.createParseBackendForTesting(
        nativeLibraryPathOverride:
            '/definitely/missing/libsovereign_comrak_bridge.dylib',
      ),
      throwsA(isA<NativeComrakBridgeLoadException>()),
    );
  });

  test('controller defaults to commonMark profile', () {
    final controller = SovereignController();
    addTearDown(controller.dispose);

    expect(controller.markdownProfile, MarkdownSyntaxProfile.commonMarkCore);
  });

  test('controller supports explicit markdown profile selection', () {
    final controller = SovereignController(
      markdownProfile: MarkdownSyntaxProfile.commonMarkGfm,
    );
    addTearDown(controller.dispose);

    expect(controller.markdownProfile, MarkdownSyntaxProfile.commonMarkGfm);
  });
}
