import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/engine/native_comrak_bridge_factory.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/native_comrak_ffi.dart';
import 'support/test_paths.dart';

void main() {
  group('NativeComrakPayloadCodec', () {
    test('decodes payload into parse result model', () {
      final payload = utf8.encode(
        jsonEncode({
          'blocks': [
            {
              'type': 'header',
              'startByte': 0,
              'endByte': 5,
              'payload': {'level': 1},
            },
          ],
          'inlineTokens': [
            {
              'styles': ['bold', 'italic'],
              'startByte': 2,
              'endByte': 4,
            },
          ],
          'markerRanges': [
            {'startByte': 0, 'endByte': 1},
          ],
          'exclusionRanges': [
            {'startByte': 10, 'endByte': 20},
          ],
          'diagnostics': [
            {
              'startByte': 0,
              'endByte': 0,
              'message': 'warn',
              'code': 'WARN',
              'isError': false,
            },
          ],
        }),
      );

      final result = NativeComrakPayloadCodec.decode(
        revision: 11,
        payload: payload,
      );

      expect(result.revision, 11);
      expect(result.blocks, hasLength(1));
      expect(result.blocks.first.type, 'header');
      expect(result.blocks.first.payload['level'], 1);
      expect(result.inlineTokens, hasLength(1));
      expect(result.inlineTokens.first.styles, {'bold', 'italic'});
      expect(result.markerRanges, [
        const NativeComrakRange(startByte: 0, endByte: 1),
      ]);
      expect(result.exclusionRanges, [
        const NativeComrakRange(startByte: 10, endByte: 20),
      ]);
      expect(result.diagnostics, hasLength(1));
      expect(result.diagnostics.first.code, 'WARN');
    });

    test('empty payload yields empty parse result', () {
      final result = NativeComrakPayloadCodec.decode(
        revision: 7,
        payload: utf8.encode(''),
      );
      expect(result.revision, 7);
      expect(result.blocks, isEmpty);
      expect(result.inlineTokens, isEmpty);
      expect(result.markerRanges, isEmpty);
      expect(result.exclusionRanges, isEmpty);
      expect(result.diagnostics, isEmpty);
    });

    test('throws FormatException when payload root is not a JSON object', () {
      expect(
        () => NativeComrakPayloadCodec.decode(
          revision: 1,
          payload: utf8.encode(jsonEncode(<Object?>['not-an-object'])),
        ),
        throwsA(isA<FormatException>()),
      );
    });

    test('coerces numeric byte offsets and ignores malformed list entries', () {
      final payload = utf8.encode(
        jsonEncode({
          'blocks': [
            {
              'type': 'paragraph',
              'startByte': 2.9,
              'endByte': 8.1,
              'payload': {'flag': true},
            },
            'skip-me',
          ],
          'inlineTokens': [
            {
              'styles': ['bold', 7, null, 'italic'],
              'startByte': 'bad',
              'endByte': 5.6,
            },
            123,
          ],
          'markerRanges': [
            {'startByte': 4.2, 'endByte': 6.9},
            false,
          ],
          'exclusionRanges': [
            {'startByte': null, 'endByte': 12},
            {},
          ],
          'diagnostics': [
            {
              'startByte': 0,
              'endByte': 0,
              'message': 'diag',
              'code': 'D',
              'isError': true,
            },
            'skip',
          ],
        }),
      );

      final result = NativeComrakPayloadCodec.decode(
        revision: 2,
        payload: payload,
      );

      expect(result.blocks, hasLength(1));
      expect(
        result.blocks.first,
        const NativeComrakBlockSpan(
          type: 'paragraph',
          range: NativeComrakRange(startByte: 2, endByte: 8),
          payload: {'flag': true},
        ),
      );

      expect(result.inlineTokens, hasLength(1));
      expect(
        result.inlineTokens.first,
        const NativeComrakInlineToken(
          range: NativeComrakRange(startByte: 0, endByte: 5),
          styles: {'bold', 'italic'},
        ),
      );

      expect(result.markerRanges, const [
        NativeComrakRange(startByte: 4, endByte: 6),
      ]);
      expect(result.exclusionRanges, const [
        NativeComrakRange(startByte: 0, endByte: 12),
        NativeComrakRange(startByte: 0, endByte: 0),
      ]);
      expect(result.diagnostics, const [
        NativeComrakDiagnostic(
          range: NativeComrakRange(startByte: 0, endByte: 0),
          message: 'diag',
          code: 'D',
          isError: true,
        ),
      ]);
    });
  });

  group('NativeComrak value semantics', () {
    test('models compare by value and expose stable hash codes', () {
      const blockA = NativeComrakBlockSpan(
        type: 'header',
        range: NativeComrakRange(startByte: 1, endByte: 3),
        payload: {'level': 1},
      );
      const blockB = NativeComrakBlockSpan(
        type: 'header',
        range: NativeComrakRange(startByte: 1, endByte: 3),
        payload: {'level': 1},
      );
      const tokenA = NativeComrakInlineToken(
        range: NativeComrakRange(startByte: 5, endByte: 7),
        styles: {'bold', 'italic'},
      );
      const tokenB = NativeComrakInlineToken(
        range: NativeComrakRange(startByte: 5, endByte: 7),
        styles: {'italic', 'bold'},
      );

      expect(blockA, blockB);
      expect(blockA.hashCode, blockB.hashCode);
      expect(tokenA, tokenB);
      expect(tokenA.hashCode, tokenB.hashCode);
    });
  });

  group('createNativeComrakBridge', () {
    test('throws when library is missing', () {
      expect(
        () => createNativeComrakBridge(
          overrideLibraryPath:
              '/definitely/missing/libsovereign_comrak_bridge.so',
        ),
        throwsA(
          isA<NativeComrakBridgeLoadException>()
              .having(
                (e) => e.kind,
                'kind',
                NativeComrakBridgeLoadFailureKind.libraryNotFound,
              )
              .having(
                (e) => e.remediationSteps.join('\n'),
                'remediation',
                contains('build hook'),
              ),
        ),
      );
    });

    test(
      'preflight reports actionable error when override path is missing',
      () {
        final result = preflightNativeComrakBridge(
          overrideLibraryPath:
              '/definitely/missing/libsovereign_comrak_bridge.so',
        );

        expect(result.isAvailable, isFalse);
        expect(result.error, isNotNull);
        expect(
          result.error!.kind,
          NativeComrakBridgeLoadFailureKind.libraryNotFound,
        );
        expect(
          result.error!.toString(),
          allOf(
            contains('NativeComrakBridgeLoadException'),
            contains('build hook'),
            contains('build_comrak_all.sh --strict'),
          ),
        );
      },
    );

    test('loads compiled bridge and parses markdown payload', () async {
      final libPath = sovereignNativeBridgeLibraryPathForPlatform();
      if (libPath.isEmpty || !File(libPath).existsSync()) {
        // Local test environments may not have built native artifacts.
        return;
      }

      final bridge = createNativeComrakBridge(overrideLibraryPath: libPath);
      final result = await bridge.parse(
        NativeComrakParseInput(
          revision: 22,
          profile: NativeComrakProfile.commonMarkCore,
          utf8Text: Uint8List.fromList(utf8.encode('# Title\n\n`code`\n')),
        ),
      );

      expect(result.revision, 22);
      expect(result.blocks.any((block) => block.type == 'header'), isTrue);
      expect(
        result.inlineTokens.any((token) => token.styles.contains('code')),
        isTrue,
      );
      expect(
        result.markerRanges,
        contains(const NativeComrakRange(startByte: 0, endByte: 2)),
      );
      expect(
        result.markerRanges,
        contains(const NativeComrakRange(startByte: 9, endByte: 10)),
      );
      expect(
        result.markerRanges,
        contains(const NativeComrakRange(startByte: 14, endByte: 15)),
      );
      expect(result.diagnostics, isEmpty);
    });

    test('inline marker extraction respects fenced-code exclusions', () async {
      final libPath = sovereignNativeBridgeLibraryPathForPlatform();
      if (libPath.isEmpty || !File(libPath).existsSync()) {
        return;
      }

      final bridge = createNativeComrakBridge(overrideLibraryPath: libPath);
      final result = await bridge.parse(
        NativeComrakParseInput(
          revision: 23,
          profile: NativeComrakProfile.commonMarkCore,
          utf8Text: Uint8List.fromList(utf8.encode('```\n**x**\n```\n**y**\n')),
        ),
      );

      expect(
        result.markerRanges,
        contains(const NativeComrakRange(startByte: 0, endByte: 3)),
      );
      expect(
        result.markerRanges,
        contains(const NativeComrakRange(startByte: 10, endByte: 13)),
      );
      expect(
        result.markerRanges,
        contains(const NativeComrakRange(startByte: 14, endByte: 16)),
      );
      expect(
        result.markerRanges,
        contains(const NativeComrakRange(startByte: 17, endByte: 19)),
      );
      expect(
        result.markerRanges,
        isNot(contains(const NativeComrakRange(startByte: 4, endByte: 6))),
      );
      expect(
        result.markerRanges,
        isNot(contains(const NativeComrakRange(startByte: 7, endByte: 9))),
      );
      expect(result.diagnostics, isEmpty);
    });

    test(
      'supported fenced code info tags are emitted as hidden markers',
      () async {
        final libPath = sovereignNativeBridgeLibraryPathForPlatform();
        if (libPath.isEmpty || !File(libPath).existsSync()) {
          return;
        }

        final bridge = createNativeComrakBridge(overrideLibraryPath: libPath);
        final result = await bridge.parse(
          NativeComrakParseInput(
            revision: 24,
            profile: NativeComrakProfile.commonMarkCore,
            utf8Text: Uint8List.fromList(utf8.encode('```dart\nx\n```\n')),
          ),
        );

        expect(
          result.markerRanges,
          contains(const NativeComrakRange(startByte: 0, endByte: 3)),
        );
        expect(
          result.markerRanges,
          contains(const NativeComrakRange(startByte: 3, endByte: 7)),
        );
        expect(
          result.markerRanges,
          contains(const NativeComrakRange(startByte: 10, endByte: 13)),
        );
        expect(result.diagnostics, isEmpty);
      },
    );
  });
}
