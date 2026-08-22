import 'dart:convert';
import 'dart:io';

void main(List<String> arguments) {
  if (arguments.isEmpty || arguments.length.isOdd) {
    stderr.writeln(
      'Usage: dart run tool/verify_package_config.dart '
      '<package> <expected-directory> [...]',
    );
    exitCode = 64;
    return;
  }

  final configFile = File('.dart_tool/package_config.json').absolute;
  final config =
      jsonDecode(configFile.readAsStringSync()) as Map<String, Object?>;
  final packages = (config['packages']! as List<Object?>)
      .cast<Map<String, Object?>>();

  for (var index = 0; index < arguments.length; index += 2) {
    final name = arguments[index];
    final expectedDirectory = Directory(arguments[index + 1]);
    final matches = packages
        .where((package) => package['name'] == name)
        .toList();
    if (matches.length != 1) {
      throw StateError(
        'Expected one package_config entry for $name, found ${matches.length}.',
      );
    }

    final rootUri = configFile.uri.resolve(
      matches.single['rootUri']! as String,
    );
    final actual = Directory.fromUri(rootUri).resolveSymbolicLinksSync();
    final expected = expectedDirectory.resolveSymbolicLinksSync();
    if (actual != expected) {
      throw StateError(
        '$name resolved outside the extracted archive. '
        'Expected $expected, found $actual.',
      );
    }
  }

  stdout.writeln('Resolved package roots are the extracted pub archives.');
}
