import 'dart:io';
import 'dart:typed_data';

import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  test('normal engine barrel resolves in a standalone Dart test', () {
    const extensions = FlarkExtensionSet.empty();
    final wasmUri = NativeComrakWasmUriSource(
      Uri.parse('/assets/flark_comrak_bridge.wasm'),
    );
    final wasmBytes = NativeComrakWasmBytesSource(Uint8List.fromList([0, 97]));
    final wasmLoader = NativeComrakWasmBytesLoaderSource(
      () async => Uint8List.fromList([0, 97]),
    );

    expect(extensions.extensions, isEmpty);
    expect(FlarkMarkdownProfile.commonMarkCore, isA<FlarkMarkdownProfile>());
    expect(wasmUri.uri.path, '/assets/flark_comrak_bridge.wasm');
    expect(wasmBytes.bytes, orderedEquals([0, 97]));
    expect(wasmLoader.load, isA<Future<Uint8List> Function()>());
  });

  test('engine package has no Flutter dependency or library import', () {
    final pubspec = File('pubspec.yaml').readAsStringSync();
    expect(pubspec, isNot(contains('sdk: flutter')));
    for (final dependency in const ['flutter', 'flutter_test', 'highlight']) {
      expect(
        RegExp('^  $dependency:', multiLine: true).hasMatch(pubspec),
        isFalse,
        reason: 'root pubspec must not declare $dependency',
      );
    }

    final violations = <String>[];
    final files = [
      ...Directory('lib').listSync(recursive: true).whereType<File>(),
      ...Directory('hook').listSync(recursive: true).whereType<File>(),
    ].where((file) => file.path.endsWith('.dart'));
    for (final file in files) {
      final lines = file.readAsLinesSync();
      for (var index = 0; index < lines.length; index += 1) {
        final line = lines[index].trimLeft();
        if ((line.startsWith('import ') || line.startsWith('export ')) &&
            const [
              'package:flutter/',
              'package:flark_flutter/',
              'package:highlight/',
              'dart:ui',
            ].any(line.contains)) {
          violations.add('${file.path}:${index + 1}: $line');
        }
      }
    }

    expect(violations, isEmpty, reason: violations.join('\n'));
  });

  test('Flutter adapter depends on the engine, never the reverse', () {
    final adapterPubspec = File(
      'packages/flark_flutter/pubspec.yaml',
    ).readAsStringSync();
    final adapterOverrides = File(
      'packages/flark_flutter/pubspec_overrides.yaml',
    ).readAsStringSync();
    expect(adapterPubspec, contains('flark: ^0.1.1'));
    expect(adapterPubspec, contains('sdk: flutter'));
    expect(adapterOverrides, contains('path: ../..'));
  });

  test('shipped guides preserve the Dart and Flutter package boundary', () {
    final gettingStarted = File('doc/getting_started.md').readAsStringSync();
    final cookbook = File('doc/cookbook.md').readAsStringSync();
    final apiSurface = File('doc/api_surface.md').readAsStringSync();
    final development = File('doc/development.md').readAsStringSync();
    final benchmarks = File('doc/benchmarks.md').readAsStringSync();
    final platforms = File('doc/parser_and_platforms.md').readAsStringSync();
    final exampleReadme = File('example/README.md').readAsStringSync();

    for (final flutterGuide in [gettingStarted, cookbook]) {
      expect(
        flutterGuide,
        contains("import 'package:flark_flutter/flark_flutter.dart';"),
      );
      expect(
        flutterGuide,
        isNot(contains("import 'package:flark/flark.dart';")),
      );
    }
    expect(apiSurface, contains("import 'package:flark/flark.dart';"));
    expect(
      apiSurface,
      contains("import 'package:flark_flutter/flark_flutter.dart';"),
    );
    expect(development, contains('dart pub get'));
    expect(development, contains('cd packages/flark_flutter'));
    expect(benchmarks, contains('cd packages/flark_flutter'));
    expect(platforms, contains('Dart web applications own the URL or bytes'));
    expect(platforms, contains('long-lived Web Worker'));
    expect(exampleReadme, contains('package:flark_flutter/flark_flutter.dart'));
  });
}
