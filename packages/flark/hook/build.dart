// Build hook for the flark_parse code asset.
//
// Resolution order, so a consumer without a Rust toolchain still builds. The
// hook runner sanitizes the environment (PATH, HOME, and the Android NDK
// variables pass through; nothing else does), so locations are files or
// pubspec user-defines, never ad-hoc environment variables.
//   1. prebuilt/<triple>/<library> bundled inside this package (release
//      packaging fills it; absent in a source checkout).
//   2. The consumer's pubspec user-define `hooks: user_defines: flark:
//      prebuilt_dir: <dir>` holding <dir>/<triple>/<library>.
//   3. The crate at native/flark_parse (repo checkouts) built with cargo on
//      the toolchain named by the repository's rust-toolchain.toml, through
//      rustup when present so cross targets resolve.
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
      final dir = userDir.toFilePath();
      final source = File('$dir${dir.endsWith('/') ? '' : '/'}${plan.triple}/${plan.libraryFileName}');
      if (!source.existsSync()) throw BuildError(message: 'flark_parse: the prebuilt_dir user-define is set but ${source.path} is missing');
      output.dependencies.add(source.uri);
      artifact = source.uri;
    } else if (Directory.fromUri(crateRoot).existsSync()) {
      for (final f in Directory.fromUri(crateRoot.resolve('src/')).listSync(recursive: true).whereType<File>()) { output.dependencies.add(f.uri); }
      for (final name in ['Cargo.toml', 'Cargo.lock', '../../rust-toolchain.toml']) {
        final f = File.fromUri(crateRoot.resolve(name));
        if (f.existsSync()) output.dependencies.add(f.uri);
      }
      artifact = await _cargoBuild(plan, crateRoot, outputDir);
    } else {
      throw BuildError(message: 'flark_parse: no bundled prebuilt, no `prebuilt_dir` user-define, and no crate at ${crateRoot.toFilePath()}. Add to your pubspec:\n  hooks:\n    user_defines:\n      flark:\n        prebuilt_dir: <directory holding ${plan.triple}/${plan.libraryFileName}>');
    }

    output.assets.code.add(CodeAsset(package: input.packageName, name: _assetName, file: artifact, linkMode: DynamicLoadingBundled()));
  });
}

Future<Uri> _cargoBuild(_Plan plan, Uri crateRoot, Uri outputDir) async {
  final targetDir = Directory.fromUri(outputDir.resolve('cargo_target/'))..createSync(recursive: true);
  final environment = <String, String>{'CARGO_TARGET_DIR': targetDir.path, ...plan.environment()};
  final rustup = await _which('rustup');
  final List<String> command;
  if (rustup != null) {
    // The toolchain is whatever rust-toolchain.toml selects for the crate
    // directory, so the hook, CI, and the wasm build agree on one compiler.
    final active = await Process.run(rustup, ['show', 'active-toolchain'], workingDirectory: crateRoot.toFilePath());
    final toolchain = active.stdout.toString().trim().split(RegExp(r'\s+')).first;
    if (active.exitCode != 0 || toolchain.isEmpty) throw BuildError(message: 'flark_parse: rustup could not resolve a toolchain for ${crateRoot.toFilePath()}: ${active.stderr}');
    final installed = (await Process.run(rustup, ['target', 'list', '--installed', '--toolchain', toolchain])).stdout.toString();
    if (!installed.split('\n').map((l) => l.trim()).contains(plan.triple)) {
      final add = await Process.run(rustup, ['target', 'add', plan.triple, '--toolchain', toolchain]);
      if (add.exitCode != 0) throw BuildError(message: 'flark_parse: `rustup target add ${plan.triple} --toolchain $toolchain` failed:\n${add.stderr}');
    }
    final rustc = (await Process.run(rustup, ['which', 'rustc', '--toolchain', toolchain])).stdout.toString().trim();
    if (rustc.isNotEmpty) environment['RUSTC'] = rustc;
    command = [rustup, 'run', toolchain, 'cargo'];
  } else {
    final cargo = await _which('cargo');
    if (cargo == null) throw BuildError(message: 'flark_parse: neither rustup nor cargo found. Install Rust (rustup.rs) or point the `prebuilt_dir` user-define at prebuilt libraries.');
    command = [cargo];
  }
  final result = await Process.run(command.first, [...command.skip(1), 'build', '--release', '--locked', '--lib', '--manifest-path', crateRoot.resolve('Cargo.toml').toFilePath(), '--target', plan.triple], environment: environment);
  if (result.exitCode != 0) throw BuildError(message: 'flark_parse: cargo build failed\n${result.stdout}\n${result.stderr}');
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
  final Map<String, String> Function() environment;

  static _Plan? resolve(CodeConfig code) {
    final os = code.targetOS;
    final arch = code.targetArchitecture;
    if (os == OS.macOS) {
      final t = switch (arch) { Architecture.arm64 => 'aarch64-apple-darwin', Architecture.x64 => 'x86_64-apple-darwin', _ => null };
      return t == null ? null : _Plan(t, 'libflark_parse.dylib', () => const {});
    }
    if (os == OS.linux) {
      final t = switch (arch) { Architecture.arm64 => 'aarch64-unknown-linux-gnu', Architecture.x64 => 'x86_64-unknown-linux-gnu', _ => null };
      return t == null ? null : _Plan(t, 'libflark_parse.so', () => const {});
    }
    if (os == OS.iOS) {
      final simulator = code.iOS.targetSdk == IOSSdk.iPhoneSimulator;
      final t = switch (arch) { Architecture.arm64 => simulator ? 'aarch64-apple-ios-sim' : 'aarch64-apple-ios', Architecture.x64 => 'x86_64-apple-ios', _ => null };
      if (t == null) return null;
      final sdk = simulator ? 'iphonesimulator' : 'iphoneos';
      return _Plan(t, 'libflark_parse.dylib', () => {..._appleEnvironment(sdk, t), 'IPHONEOS_DEPLOYMENT_TARGET': '${code.iOS.targetVersion}.0'});
    }
    if (os == OS.android) {
      final (triple, linkerPrefix) = switch (arch) {
        Architecture.arm64 => ('aarch64-linux-android', 'aarch64-linux-android'),
        Architecture.arm => ('armv7-linux-androideabi', 'armv7a-linux-androideabi'),
        Architecture.x64 => ('x86_64-linux-android', 'x86_64-linux-android'),
        _ => (null, null),
      };
      if (triple == null) return null;
      return _Plan(triple, 'libflark_parse.so', () => _androidEnvironment(triple, linkerPrefix!, code.android.targetNdkApi));
    }
    return null;
  }
}

/// Apple cross builds need the SDK sysroot and its clang as the linker.
Map<String, String> _appleEnvironment(String sdk, String triple) {
  final clang = _xcrun(sdk, ['--find', 'clang']);
  final sdkRoot = _xcrun(sdk, ['--show-sdk-path']);
  final upper = triple.toUpperCase().replaceAll('-', '_');
  final snake = triple.replaceAll('-', '_');
  return {
    'SDKROOT': sdkRoot,
    'CARGO_TARGET_${upper}_LINKER': clang,
    'CC_$snake': clang,
    'CFLAGS_$snake': '-isysroot $sdkRoot',
  };
}

String _xcrun(String sdk, List<String> arguments) {
  final result = Process.runSync('/usr/bin/xcrun', ['--sdk', sdk, ...arguments]);
  final value = (result.stdout as String).trim();
  if (result.exitCode == 0 && value.isNotEmpty) return value;
  throw BuildError(message: 'flark_parse: xcrun could not resolve the $sdk toolchain: ${result.stderr}');
}

/// Android cross builds link with the NDK's clang for the target API level.
Map<String, String> _androidEnvironment(String triple, String linkerPrefix, int apiLevel) {
  final ndk = _findAndroidNdk();
  if (ndk == null) throw BuildError(message: 'flark_parse: an Android build needs ANDROID_NDK_HOME, ANDROID_NDK, ANDROID_NDK_ROOT, ANDROID_NDK_LATEST_HOME, or ANDROID_HOME with an installed NDK.');
  final hostTag = switch (Platform.operatingSystem) { 'macos' => const ['darwin-arm64', 'darwin-x86_64'], 'linux' => const ['linux-x86_64'], _ => const <String>[] }
      .cast<String?>()
      .firstWhere((tag) => Directory.fromUri(ndk.uri.resolve('toolchains/llvm/prebuilt/$tag/')).existsSync(), orElse: () => null);
  if (hostTag == null) throw BuildError(message: 'flark_parse: no prebuilt LLVM toolchain under ${ndk.path}/toolchains/llvm/prebuilt.');
  final bin = ndk.uri.resolve('toolchains/llvm/prebuilt/$hostTag/bin/');
  final linker = bin.resolve('$linkerPrefix$apiLevel-clang').toFilePath();
  final ar = bin.resolve('llvm-ar').toFilePath();
  final upper = triple.toUpperCase().replaceAll('-', '_');
  final snake = triple.replaceAll('-', '_');
  return {
    'CARGO_TARGET_${upper}_LINKER': linker,
    'CARGO_TARGET_${upper}_AR': ar,
    'CC_$snake': linker,
    'AR_$snake': ar,
  };
}

Directory? _findAndroidNdk() {
  for (final key in ['ANDROID_NDK_HOME', 'ANDROID_NDK', 'ANDROID_NDK_ROOT', 'ANDROID_NDK_LATEST_HOME']) {
    final path = Platform.environment[key];
    if (path != null && path.isNotEmpty && Directory(path).existsSync()) return Directory(path);
  }
  final home = Platform.environment['ANDROID_HOME'];
  if (home == null || home.isEmpty) return null;
  final root = Directory.fromUri(Directory(home).uri.resolve('ndk/'));
  if (!root.existsSync()) return null;
  final ndks = root.listSync().whereType<Directory>().toList()..sort((a, b) => a.path.compareTo(b.path));
  return ndks.isEmpty ? null : ndks.last;
}
