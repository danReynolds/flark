import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter_test/flutter_test.dart';

import '../support/flark_test_paths.dart';

void main() {
  testWidgets(
    'threshold escape hatch forces synchronous parsing under fake async',
    (_) async {
      final libPath = flarkNativeBridgeLibraryPathForPlatform();
      if (libPath.isEmpty || !File(libPath).existsSync()) {
        return;
      }

      final previous = flarkNativeParseIsolateThresholdBytes;
      flarkNativeParseIsolateThresholdBytes = 1 << 30;
      addTearDown(() {
        flarkNativeParseIsolateThresholdBytes = previous;
      });

      final bridge = createNativeComrakBridge(overrideLibraryPath: libPath);
      final markdown = StringBuffer();
      for (var i = 0; i < 200; i++) {
        markdown.writeln('paragraph $i with enough text to cross 4 KiB');
      }
      // Without the raised threshold this parse routes through Isolate.run,
      // whose reply port never fires inside flutter_test's fake-async zone —
      // this await would hang until the test times out.
      final result = await bridge.parse(
        NativeComrakParseInput(
          revision: 1,
          profile: NativeComrakProfile.commonMarkGfm,
          utf8Text: Uint8List.fromList(utf8.encode(markdown.toString())),
        ),
      );

      expect(result.revision, 1);
      expect(result.blocks, isNotEmpty);
    },
  );
}
