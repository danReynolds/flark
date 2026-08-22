import 'dart:io';

import 'package:flark/src/v3/runtime/native/flark_v3_native_library_locator.dart';
import 'package:test/test.dart';

void main() {
  test('relocatable Dart CLI bundle searches its sibling lib directory', () {
    final candidates = flarkV3NativeLibraryCandidates(
      libraryName: 'libflark_comrak_bridge.dylib',
      executable: File('/opt/flark/bundle/bin/flark_cli'),
      currentDirectory: Directory('/unrelated/working/directory'),
      includeMacFrameworks: false,
    );

    expect(
      candidates,
      contains('/opt/flark/bundle/lib/libflark_comrak_bridge.dylib'),
    );
  });

  test('development candidates derive only from the supplied working tree', () {
    final candidates = flarkV3NativeLibraryCandidates(
      libraryName: 'libflark_comrak_bridge.so',
      executable: File('/opt/flark/bundle/bin/flark_cli'),
      currentDirectory: Directory('/work/consumer'),
      includeMacFrameworks: false,
    );

    expect(
      candidates,
      contains('/work/consumer/.dart_tool/lib/libflark_comrak_bridge.so'),
    );
    expect(
      candidates,
      contains(
        '/work/consumer/native/comrak_bridge/target/release/'
        'libflark_comrak_bridge.so',
      ),
    );
  });
}
