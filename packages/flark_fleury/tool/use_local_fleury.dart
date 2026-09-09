import 'dart:convert';
import 'dart:io';

/// Development setup for unpublished sibling packages. Generates only ignored
/// overrides; no local absolute paths belong in a package's pubspec.yaml.
void main(List<String> args) {
  if (args.length != 1) {
    stderr.writeln('Usage: dart tool/use_local_fleury.dart /path/to/fleury');
    exitCode = 64;
    return;
  }
  final root = Directory(args.single).absolute.path;
  for (final name in ['fleury', 'fleury_web', 'fleury_widgets']) {
    if (!File('$root/packages/$name/pubspec.yaml').existsSync()) {
      throw ArgumentError('Missing $name under $root/packages');
    }
  }
  final host = File.fromUri(Platform.script).parent.parent;
  for (final (directory, names) in [
    (host.path, ['fleury', 'fleury_widgets']),
    ('${host.path}/example', ['fleury', 'fleury_web', 'fleury_widgets']),
  ]) {
    File('$directory/pubspec_overrides.yaml').writeAsStringSync(
      'dependency_overrides:\n${names.map((name) => '  $name:\n    path: ${jsonEncode('$root/packages/$name')}\n').join()}',
    );
  }
  stdout.writeln(
    'Wrote local overrides. Run dart pub get in this package and example/.',
  );
}
