// Build hook for the flark_parse code asset.
//
// Resolution order, so a consumer without a Rust toolchain still builds. The
// hook runner sanitizes the environment, so environment variables are not a
// channel; locations are files or pubspec user-defines.
//   1. prebuilt/<triple>/<library> bundled inside this package (release
//      packaging fills it; absent in a source checkout).
//   2. The consumer's pubspec user-define `hooks: user_defines: flark:
//      prebuilt_dir: <dir>` holding <dir>/<triple>/<library>.
//   3. The crate at native/flark_parse (repo checkouts) built with cargo,
//      through rustup when present so cross targets resolve.
import 'dart:io';

import 'package:code_assets/code_assets.dart';
import 'package:hooks/hooks.dart';

const _assetName = 'src/parse/native.dart';
const _crateRelativePath = '../../native/flark_parse';

void main(List<String> args) async {
  await build(args, (input, output) async {
    if (!input.config.buildCodeAssets) return;
    final code = input.config.code;
    final plan = _Plan.resolve(code);
    if (plan == null) {
      throw BuildError(message: 'flark_parse: unsupported target ${code.targetOS}/${code.targetArchitecture}');
    }
    final packageRoot = input.packageRoot;
    final crateRoot = packageRoot.resolve('$_crateRelativePath/');
    final outputDir = input.outputDirectory;

    Uri artifact;
    final bundled = File.fromUri(packageRoot.resolve('prebuilt/${plan.triple}/${plan.libraryFileName}'));
    final userDir = input.userDefines.path('prebuilt_dir');
    if (bundled.existsSync()) {
      output.dependencies.add(bundled.uri);
      artifact = bundled.uri;
    } else if (userDir != null) {
      final source = File('${userDir.toFilePath()}${userDir.toFilePath().endsWith('/') ? '' : '/'}${plan.triple}/${plan.libraryFileName}');
      if (!source.existsSync()) throw BuildError(message: 'flark_parse: prebuilt_dir user-define set but ${source.path} is missing');
      output.dependencies.add(source.uri);
      artifact = source.uri;
    } else if (Directory.fromUri(crateRoot).existsSync()) {
      for (final f in Directory.fromUri(crateRoot.resolve('src/')).listSync(recursive: true).whereType<File>()) { output.dependencies.add(f.uri); }
      output.dependencies.add(crateRoot.resolve('Cargo.toml'));
      artifact = await _cargoBuild(plan, crateRoot, outputDir);
    } else {
      throw BuildError(message: 'flark_parse: no bundled prebuilt, no prebuilt_dir user-define, and no crate at ${crateRoot.toFilePath()}');
    }

    output.assets.code.add(CodeAsset(package: input.packageName, name: _assetName, file: artifact, linkMode: DynamicLoadingBundled()));
  });
}

Future<Uri> _cargoBuild(_Plan plan, Uri crateRoot, Uri outputDir) async {
  final targetDir = Directory.fromUri(outputDir.resolve('cargo_target/'))..createSync(recursive: true);
  final rustup = await _which('rustup');
  final List<String> command;
  final environment = <String, String>{'CARGO_TARGET_DIR': targetDir.path, ...plan.environment};
  if (rustup != null) {
    final rustc = (await Process.run(rustup, ['which', 'rustc', '--toolchain', 'stable'])).stdout.toString().trim();
    if (rustc.isNotEmpty) environment['RUSTC'] = rustc;
    final installed = (await Process.run(rustup, ['target', 'list', '--installed', '--toolchain', 'stable'])).stdout.toString();
    if (!installed.contains(plan.triple)) {
      final add = await Process.run(rustup, ['target', 'add', plan.triple, '--toolchain', 'stable']);
      if (add.exitCode != 0) throw BuildError(message: 'flark_parse: rustup target add ${plan.triple} failed: ${add.stderr}');
    }
    command = [rustup, 'run', 'stable', 'cargo'];
  } else {
    final cargo = await _which('cargo');
    if (cargo == null) throw BuildError(message: 'flark_parse: neither rustup nor cargo found; set FLARK_PARSE_PREBUILT_DIR or install Rust');
    command = [cargo];
  }
  final result = await Process.run(command.first, [...command.skip(1), 'build', '--release', '--lib', '--manifest-path', crateRoot.resolve('Cargo.toml').toFilePath(), '--target', plan.triple], environment: environment);
  if (result.exitCode != 0) throw BuildError(message: 'flark_parse: cargo build failed\n${result.stderr}');
  final built = File(targetDir.uri.resolve('${plan.triple}/release/${plan.libraryFileName}').toFilePath());
  if (!built.existsSync()) throw BuildError(message: 'flark_parse: expected artifact missing: ${built.path}');
  return built.uri;
}

Future<String?> _which(String name) async {
  final r = await Process.run(Platform.isWindows ? 'where' : 'which', [name]);
  if (r.exitCode != 0) return null;
  final s = r.stdout.toString().trim().split('\n').first.trim();
  return s.isEmpty ? null : s;
}

class _Plan {
  _Plan(this.triple, this.libraryFileName, this.environment);
  final String triple;
  final String libraryFileName;
  final Map<String, String> environment;

  static _Plan? resolve(CodeConfig code) {
    final os = code.targetOS;
    final arch = code.targetArchitecture;
    if (os == OS.macOS) {
      final t = switch (arch) { Architecture.arm64 => 'aarch64-apple-darwin', Architecture.x64 => 'x86_64-apple-darwin', _ => null };
      return t == null ? null : _Plan(t, 'libflark_parse.dylib', const {});
    }
    if (os == OS.linux) {
      final t = switch (arch) { Architecture.arm64 => 'aarch64-unknown-linux-gnu', Architecture.x64 => 'x86_64-unknown-linux-gnu', _ => null };
      return t == null ? null : _Plan(t, 'libflark_parse.so', const {});
    }
    if (os == OS.iOS) {
      final simulator = code.iOS.targetSdk == IOSSdk.iPhoneSimulator;
      final t = switch (arch) { Architecture.arm64 => simulator ? 'aarch64-apple-ios-sim' : 'aarch64-apple-ios', Architecture.x64 => 'x86_64-apple-ios', _ => null };
      return t == null ? null : _Plan(t, 'libflark_parse.dylib', const {});
    }
    if (os == OS.android) {
      final t = switch (arch) { Architecture.arm64 => 'aarch64-linux-android', Architecture.arm => 'armv7-linux-androideabi', Architecture.x64 => 'x86_64-linux-android', _ => null };
      return t == null ? null : _Plan(t, 'libflark_parse.so', const {});
    }
    return null;
  }
}
