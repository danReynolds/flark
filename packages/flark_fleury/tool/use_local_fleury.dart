import 'dart:convert';
import 'dart:io';

/// Development setup against a local Fleury checkout.
///
/// The packages resolve as one pub workspace. Pub applies any member's
/// overrides to the whole workspace but lets each package be overridden only
/// once, so this writes the root's `pubspec_overrides.yaml`. That file is
/// ignored by Git; delete it to return to Fleury from pub.dev. No local
/// absolute path belongs in a tracked pubspec.
void main(List<String> args) {
  if (args.length != 1) {
    stderr.writeln('Usage: dart tool/use_local_fleury.dart /path/to/fleury');
    exitCode = 64;
    return;
  }
  final fleury = Directory(args.single).absolute.path;
  const names = ['fleury', 'fleury_web'];
  for (final name in names) {
    if (!File('$fleury/packages/$name/pubspec.yaml').existsSync()) {
      stderr.writeln('Missing $name under $fleury/packages');
      exitCode = 66;
      return;
    }
  }
  final package = File.fromUri(Platform.script).parent.parent;
  final root = package.parent.parent;
  if (!File(
    '${root.path}/pubspec.yaml',
  ).readAsStringSync().contains('\nworkspace:')) {
    stderr.writeln('${root.path} is not the Flark workspace root');
    exitCode = 66;
    return;
  }
  for (final member in [package.path, '${package.path}/example']) {
    if (File('$member/pubspec_overrides.yaml').existsSync()) {
      stderr.writeln(
        'Remove $member/pubspec_overrides.yaml first: pub refuses a package '
        'overridden twice in one workspace.',
      );
      exitCode = 65;
      return;
    }
  }
  final overrides = File('${root.path}/pubspec_overrides.yaml');
  overrides.writeAsStringSync(
    '# Written by packages/flark_fleury/tool/use_local_fleury.dart. Delete it\n'
    '# to resolve the pinned Fleury revision again.\n'
    'dependency_overrides:\n'
    '${names.map((name) => '  $name:\n    path: ${jsonEncode('$fleury/packages/$name')}\n').join()}',
  );
  stdout.writeln(
    'Wrote ${overrides.path}. Run flutter pub get in ${root.path}.',
  );
}
