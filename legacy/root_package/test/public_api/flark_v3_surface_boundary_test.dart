import 'dart:io';

import 'package:test/test.dart';

void main() {
  test('normal v3 barrel is an explicit document-facade allow-list', () {
    final barrel = File('lib/flark_v3.dart').readAsStringSync();
    final adapter = File('lib/flark_adapter.dart').readAsStringSync();

    for (final requiredLibrary in const <String>[
      'flark_v3_document_query.dart',
      'flark_v3_document_runtime.dart',
      'flark_v3_runtime_assets.dart',
      'flark_v3_source_document.dart',
    ]) {
      expect(barrel, contains(requiredLibrary));
    }

    for (final forbiddenLibrary in const <String>[
      "src/v3/host/host.dart",
      "src/v3/session/session.dart",
      "src/v3/source/source.dart",
      "src/v3/runtime/flark_v3_parser_transport.dart",
    ]) {
      expect(
        barrel,
        isNot(contains(forbiddenLibrary)),
        reason: '$forbiddenLibrary belongs to the advanced adapter SPI',
      );
    }

    expect(barrel, isNot(contains('FlarkV3ParserSessionBinding')));
    expect(barrel, isNot(contains('FlarkV3HostStore')));
    expect(barrel, isNot(contains('FlarkV3SourceSession')));
    expect(barrel, isNot(contains('FlarkDocumentSession')));
    expect(
      adapter,
      contains('runFlarkV3CheckpointBProbeJson'),
      reason: 'the private feedback probe belongs only to the adapter SPI',
    );
    expect(
      barrel,
      isNot(contains('runFlarkV3CheckpointBProbeJson')),
      reason: 'the feedback probe is not a stable product runtime API',
    );
  });

  test('normal v3 import cannot name internal assembly choreography', () async {
    final scratch = Directory('.dart_tool/flark_v3_negative_surface_test')
      ..createSync(recursive: true);
    addTearDown(() {
      if (scratch.existsSync()) scratch.deleteSync(recursive: true);
    });
    final fixture = File('${scratch.path}/internal_names.dart')
      ..writeAsStringSync('''
import 'package:flark/flark_v3.dart';

void main() {
  FlarkV3HostStore? host;
  FlarkV3ParserSessionBinding? binding;
  FlarkV3SourceSession? source;
  FlarkDocumentSession.attach;
  FlarkV3DocumentRuntime.attach;
  print((host, binding, source));
}
''');

    final analysis = await Process.run(Platform.resolvedExecutable, <String>[
      'analyze',
      fixture.path,
    ], workingDirectory: Directory.current.path);
    final output = '${analysis.stdout}\n${analysis.stderr}';
    expect(analysis.exitCode, isNot(0), reason: output);
    for (final forbiddenName in const <String>[
      'FlarkV3HostStore',
      'FlarkV3ParserSessionBinding',
      'FlarkV3SourceSession',
      'FlarkDocumentSession',
    ]) {
      expect(output, contains(forbiddenName), reason: output);
    }
    expect(output, contains("getter 'attach' isn't defined"), reason: output);
  });
}
