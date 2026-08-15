@TestOn('browser')
library;

import 'dart:typed_data';

import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  test(
    'Dart-only web consumer boots the parser from an explicit URI',
    () async {
      final backend = FlarkNativeComrakParseBackend.withNativeBridge(
        // A cross-platform configuration may carry both values. Web consumes
        // the Wasm source; native platforms consume the library override.
        overrideLibraryPath: '/ignored-native-library',
        wasmSource: NativeComrakWasmUriSource(
          Uri.base.resolve(
            'packages/flark/assets/wasm/flark_comrak_bridge.wasm',
          ),
        ),
      );
      final result = await backend.parse(
        const FlarkMarkdownParseRequest(
          revision: 7,
          profile: FlarkMarkdownProfile.commonMarkGfm,
          markdown: '# Dart web\n\n**live**',
        ),
      );

      expect(
        result.diagnostics.where(
          (diagnostic) => diagnostic.extensions['isError'] == true,
        ),
        isEmpty,
        reason: result.diagnostics
            .map((diagnostic) => '${diagnostic.code}: ${diagnostic.message}')
            .join('\n'),
      );
      expect(result.revision, 7);
      expect(result.blocks.map((block) => block.type), contains('heading'));
      expect(
        result.inlineTokens.map((token) => token.type),
        contains('strong'),
      );
    },
  );

  test('web preflight rejects invalid byte configuration', () {
    final preflight = FlarkNativeComrakParseBackend.preflight(
      wasmSource: NativeComrakWasmBytesSource(Uint8List(0)),
    );

    expect(preflight.isAvailable, isFalse);
    expect(
      preflight.error?.kind,
      NativeComrakBridgeLoadFailureKind.invalidConfiguration,
    );
  });
}
