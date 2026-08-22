import 'dart:io';

import 'package:flark/flark_advanced.dart';
import 'package:test/test.dart';

import '../v2/support/flark_test_paths.dart';

void main() {
  test('profile current whole-document bridge phases', () async {
    final libraryPath = flarkNativeBridgeLibraryPathForPlatform();
    expect(File(libraryPath).existsSync(), isTrue);
    final backend = FlarkNativeComrakParseBackend.withNativeBridge(
      overrideLibraryPath: libraryPath,
    );
    for (final target in const [100000, 1000000]) {
      final markdown = _markdownOfSize(target);
      final profiled = await backend.parseWithProfile(
        FlarkMarkdownParseRequest(
          revision: target,
          markdown: markdown,
          profile: FlarkMarkdownProfile.commonMarkGfm,
        ),
      );
      final profile = profiled.profile;
      // ignore: avoid_print
      print(
        'flark_bridge_profile bytes=${profile.inputBytes} '
        'payload=${profile.payloadBytes} total=${profile.total.inMicroseconds}us '
        'encode=${profile.utf8Encode.inMicroseconds}us '
        'bridge=${profile.bridgeTotal.inMicroseconds}us '
        'input_copy=${profile.bridgeInputCopy.inMicroseconds}us '
        'native_parse_and_payload=${profile.nativeParse.inMicroseconds}us '
        'payload_copy=${profile.payloadCopy.inMicroseconds}us '
        'decode=${profile.payloadDecode.inMicroseconds}us '
        'dart_mapping=${profile.resultMapping.inMicroseconds}us '
        'blocks=${profile.nativeBlockCount} '
        'inlines=${profile.nativeInlineTokenCount} '
        'markers=${profile.nativeMarkerRangeCount}',
      );
    }
  });
}

String _markdownOfSize(int targetLength) {
  final buffer = StringBuffer();
  var index = 0;
  while (buffer.length < targetLength) {
    buffer
      ..writeln('## Section $index')
      ..writeln()
      ..writeln(
        'Paragraph content $index with **bold**, *emphasis*, and '
        '[a link](https://example.com/$index).',
      )
      ..writeln();
    index += 1;
  }
  return buffer.toString();
}
