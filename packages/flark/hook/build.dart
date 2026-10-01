// Build hook for the flark_parse code asset.
//
// Resolution order, so a consumer without a Rust toolchain still builds. The
// hook runner sanitizes the environment (PATH, HOME, the Android NDK and proxy
// variables pass through; nothing else does), so locations are files or
// pubspec user-defines, never ad-hoc environment variables.
//   1. prebuilt/<triple>/<library> bundled inside this package (vendored
//      copies; absent in a source checkout and in the published package).
//   2. The consumer's pubspec user-define `hooks: user_defines: flark:
//      prebuilt_dir: <dir>` holding <dir>/<triple>/<library>.
//   3. The crate at native/flark_parse (repo checkouts) built with cargo on
//      the toolchain named by the repository's rust-toolchain.toml, through
//      rustup when present so cross targets resolve.
//   4. The library published for this package version, named with its
//      SHA-256 in hook/prebuilt.json: downloaded once, checked, and cached in
//      the hook's shared output directory.
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:code_assets/code_assets.dart';
import 'package:crypto/crypto.dart';
import 'package:hooks/hooks.dart';

const _assetName = 'src/parse/native.dart';
const _crateRelativePath = '../../native/flark_parse';

void main(List<String> args) async {
  await build(args, (input, output) async {
    if (!input.config.buildCodeAssets) return;
    final code = input.config.code;
    final plan = _Plan.resolve(code);
    if (plan == null) {
      throw BuildError(
        message:
            'flark_parse: unsupported target ${code.targetOS}/${code.targetArchitecture}',
      );
    }
    final packageRoot = input.packageRoot;
    final crateRoot = packageRoot.resolve('$_crateRelativePath/');
    final outputDir = input.outputDirectory;

    Uri artifact;
    final bundled = File.fromUri(
      packageRoot.resolve('prebuilt/${plan.triple}/${plan.libraryFileName}'),
    );
    final userDir = input.userDefines.path('prebuilt_dir');
    if (bundled.existsSync()) {
      output.dependencies.add(bundled.uri);
      artifact = bundled.uri;
    } else if (userDir != null) {
      final dir = userDir.toFilePath();
      final source = File(
        '$dir${dir.endsWith('/') ? '' : '/'}${plan.triple}/${plan.libraryFileName}',
      );
      if (!source.existsSync()) {
        throw BuildError(
          message:
              'flark_parse: the prebuilt_dir user-define is set but ${source.path} is missing',
        );
      }
      output.dependencies.add(source.uri);
      artifact = source.uri;
    } else if (Directory.fromUri(crateRoot.resolve('src/')).existsSync()) {
      for (final f in Directory.fromUri(
        crateRoot.resolve('src/'),
      ).listSync(recursive: true).whereType<File>()) {
        output.dependencies.add(f.uri);
      }
      for (final name in [
        'Cargo.toml',
        'Cargo.lock',
        '../../rust-toolchain.toml',
      ]) {
        final f = File.fromUri(crateRoot.resolve(name));
        if (f.existsSync()) output.dependencies.add(f.uri);
      }
      artifact = await _cargoBuild(plan, crateRoot, outputDir);
    } else {
      final manifest = File.fromUri(packageRoot.resolve('hook/prebuilt.json'));
      output.dependencies.add(manifest.uri);
      artifact = await _download(plan, manifest, input.outputDirectoryShared);
    }

    output.assets.code.add(
      CodeAsset(
        package: input.packageName,
        name: _assetName,
        file: artifact,
        linkMode: DynamicLoadingBundled(),
      ),
    );
  });
}

Future<Uri> _cargoBuild(_Plan plan, Uri crateRoot, Uri outputDir) async {
  final targetDir = Directory.fromUri(outputDir.resolve('cargo_target/'))
    ..createSync(recursive: true);
  final environment = <String, String>{
    'CARGO_TARGET_DIR': targetDir.path,
    ...plan.environment(),
  };
  final rustup = await _which('rustup');
  final List<String> command;
  if (rustup != null) {
    // The toolchain is whatever rust-toolchain.toml selects for the crate
    // directory, so the hook, CI, and the wasm build agree on one compiler.
    final active = await Process.run(rustup, [
      'show',
      'active-toolchain',
    ], workingDirectory: crateRoot.toFilePath());
    final toolchain = active.stdout
        .toString()
        .trim()
        .split(RegExp(r'\s+'))
        .first;
    if (active.exitCode != 0 || toolchain.isEmpty) {
      throw BuildError(
        message:
            'flark_parse: rustup could not resolve a toolchain for ${crateRoot.toFilePath()}: ${active.stderr}',
      );
    }
    final installed = (await Process.run(rustup, [
      'target',
      'list',
      '--installed',
      '--toolchain',
      toolchain,
    ])).stdout.toString();
    if (!installed.split('\n').map((l) => l.trim()).contains(plan.triple)) {
      final add = await Process.run(rustup, [
        'target',
        'add',
        plan.triple,
        '--toolchain',
        toolchain,
      ]);
      if (add.exitCode != 0) {
        throw BuildError(
          message:
              'flark_parse: `rustup target add ${plan.triple} --toolchain $toolchain` failed:\n${add.stderr}',
        );
      }
    }
    final rustc = (await Process.run(rustup, [
      'which',
      'rustc',
      '--toolchain',
      toolchain,
    ])).stdout.toString().trim();
    if (rustc.isNotEmpty) environment['RUSTC'] = rustc;
    command = [rustup, 'run', toolchain, 'cargo'];
  } else {
    final cargo = await _which('cargo');
    if (cargo == null) {
      throw BuildError(
        message:
            'flark_parse: neither rustup nor cargo found. Install Rust (rustup.rs) or point the `prebuilt_dir` user-define at prebuilt libraries.',
      );
    }
    command = [cargo];
  }
  final result = await Process.run(command.first, [
    ...command.skip(1),
    'build',
    '--release',
    '--locked',
    '--lib',
    '--manifest-path',
    crateRoot.resolve('Cargo.toml').toFilePath(),
    '--target',
    plan.triple,
  ], environment: environment);
  if (result.exitCode != 0) {
    throw BuildError(
      message:
          'flark_parse: cargo build failed\n${result.stdout}\n${result.stderr}',
    );
  }
  final built = File(
    targetDir.uri
        .resolve('${plan.triple}/release/${plan.libraryFileName}')
        .toFilePath(),
  );
  if (!built.existsSync()) {
    throw BuildError(
      message: 'flark_parse: expected artifact missing: ${built.path}',
    );
  }
  return built.uri;
}

/// The published library for [plan], verified against the SHA-256 that the
/// package's manifest pins and cached by that hash, so every build after the
/// first is offline. A download that fails or does not match fails the build
/// and leaves nothing behind.
Future<Uri> _download(_Plan plan, File manifest, Uri sharedDir) async {
  String offline(String reason) =>
      'flark_parse: $reason\nTo build without downloading, point the hook at '
      'local libraries in your pubspec:\n  hooks:\n    user_defines:\n      '
      'flark:\n        prebuilt_dir: <directory holding '
      '${plan.triple}/${plan.libraryFileName}>';
  if (!manifest.existsSync()) {
    throw BuildError(message: offline('${manifest.path} is missing.'));
  }
  final files =
      (jsonDecode(manifest.readAsStringSync()) as Map<String, Object?>)['files']
          as Map<String, Object?>;
  final entry = files[plan.triple] as Map<String, Object?>?;
  if (entry == null) {
    throw BuildError(
      message: offline(
        'this flark version publishes no parser library for ${plan.triple}.',
      ),
    );
  }
  final url = Uri.parse(entry['url']! as String);
  final expected = (entry['sha256']! as String).toLowerCase();
  final cached = File.fromUri(
    sharedDir.resolve('prebuilt/$expected/${plan.libraryFileName}'),
  );
  if (cached.existsSync() &&
      sha256.convert(cached.readAsBytesSync()).toString() == expected) {
    return cached.uri;
  }
  final library = await _fetch(url, offline);
  final actual = sha256.convert(library).toString();
  if (actual != expected) {
    throw BuildError(
      message:
          'flark_parse: $url has SHA-256 $actual, but this flark version pins $expected. Nothing was used.',
    );
  }
  // Only a verified library is written, and it is renamed into place, so an
  // interrupted build cannot leave a truncated file under the cache's name.
  final partial = File('${cached.path}.partial');
  try {
    cached.parent.createSync(recursive: true);
    partial.writeAsBytesSync(library, flush: true);
    partial.renameSync(cached.path);
  } on FileSystemException catch (error) {
    if (partial.existsSync()) partial.deleteSync();
    throw BuildError(
      message: 'flark_parse: could not cache the parser library: $error',
    );
  }
  return cached.uri;
}

/// Above any parser library's size, to bound the memory a misbehaving server
/// can make a download hold.
const _maxLibraryBytes = 64 * 1024 * 1024;

/// The body of a successful GET of [url], held in memory: a parser library is
/// a few megabytes, and a download that fails then writes nothing at all.
Future<Uint8List> _fetch(Uri url, String Function(String) offline) async {
  // Hook runners put no deadline on a hook, so a stalled server must not hang
  // the build: each wait below, including the gap between response chunks,
  // is bounded.
  const stall = Duration(seconds: 60);
  final client = HttpClient()
    ..findProxy = HttpClient.findProxyFromEnvironment
    ..connectionTimeout = const Duration(seconds: 30);
  try {
    final request = await client.getUrl(url).timeout(stall);
    final response = await request.close().timeout(stall);
    if (response.statusCode != HttpStatus.ok) {
      // The body is not read: an error page can stall like any other, and
      // closing the client below abandons the connection.
      throw BuildError(
        message: offline(
          'downloading $url failed with HTTP ${response.statusCode}.',
        ),
      );
    }
    final body = BytesBuilder(copy: false);
    await for (final chunk in response.timeout(stall)) {
      body.add(chunk);
      if (body.length > _maxLibraryBytes) {
        throw BuildError(
          message: offline(
            '$url sent more than the $_maxLibraryBytes bytes a parser '
            'library can be.',
          ),
        );
      }
    }
    return body.takeBytes();
  } on TimeoutException {
    throw BuildError(
      message: offline(
        'downloading $url stalled for ${stall.inSeconds} seconds.',
      ),
    );
  } on IOException catch (error) {
    // Sockets, HTTP and TLS (a proxy that intercepts TLS, say) all land here.
    throw BuildError(message: offline('downloading $url failed: $error.'));
  } finally {
    client.close(force: true);
  }
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
      final t = switch (arch) {
        Architecture.arm64 => 'aarch64-apple-darwin',
        Architecture.x64 => 'x86_64-apple-darwin',
        _ => null,
      };
      return t == null
          ? null
          : _Plan(
              t,
              'libflark_parse.dylib',
              () => {
                ..._appleEnvironment('macosx', t),
                'MACOSX_DEPLOYMENT_TARGET': '${code.macOS.targetVersion}.0',
              },
            );
    }
    if (os == OS.windows) {
      final t = switch (arch) {
        Architecture.x64 => 'x86_64-pc-windows-msvc',
        Architecture.arm64 => 'aarch64-pc-windows-msvc',
        _ => null,
      };
      return t == null ? null : _Plan(t, 'flark_parse.dll', () => const {});
    }
    if (os == OS.linux) {
      final t = switch (arch) {
        Architecture.arm64 => 'aarch64-unknown-linux-gnu',
        Architecture.x64 => 'x86_64-unknown-linux-gnu',
        _ => null,
      };
      return t == null ? null : _Plan(t, 'libflark_parse.so', () => const {});
    }
    if (os == OS.iOS) {
      final simulator = code.iOS.targetSdk == IOSSdk.iPhoneSimulator;
      final t = switch (arch) {
        Architecture.arm64 =>
          simulator ? 'aarch64-apple-ios-sim' : 'aarch64-apple-ios',
        Architecture.x64 => 'x86_64-apple-ios',
        _ => null,
      };
      if (t == null) return null;
      final sdk = simulator ? 'iphonesimulator' : 'iphoneos';
      return _Plan(
        t,
        'libflark_parse.dylib',
        () => {
          ..._appleEnvironment(sdk, t),
          'IPHONEOS_DEPLOYMENT_TARGET': '${code.iOS.targetVersion}.0',
        },
      );
    }
    if (os == OS.android) {
      final (triple, linkerPrefix) = switch (arch) {
        Architecture.arm64 => (
          'aarch64-linux-android',
          'aarch64-linux-android',
        ),
        Architecture.arm => (
          'armv7-linux-androideabi',
          'armv7a-linux-androideabi',
        ),
        Architecture.x64 => ('x86_64-linux-android', 'x86_64-linux-android'),
        _ => (null, null),
      };
      if (triple == null) return null;
      return _Plan(
        triple,
        'libflark_parse.so',
        () => _androidEnvironment(
          triple,
          linkerPrefix!,
          code.android.targetNdkApi,
        ),
      );
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
  final result = Process.runSync('/usr/bin/xcrun', [
    '--sdk',
    sdk,
    ...arguments,
  ]);
  final value = (result.stdout as String).trim();
  if (result.exitCode == 0 && value.isNotEmpty) return value;
  throw BuildError(
    message:
        'flark_parse: xcrun could not resolve the $sdk toolchain: ${result.stderr}',
  );
}

/// Android cross builds link with the NDK's clang for the target API level.
Map<String, String> _androidEnvironment(
  String triple,
  String linkerPrefix,
  int apiLevel,
) {
  final ndk = _findAndroidNdk();
  if (ndk == null) {
    throw BuildError(
      message:
          'flark_parse: an Android build needs ANDROID_NDK_HOME, ANDROID_NDK, ANDROID_NDK_ROOT, ANDROID_NDK_LATEST_HOME, or ANDROID_HOME with an installed NDK.',
    );
  }
  final hostTag =
      switch (Platform.operatingSystem) {
        'macos' => const ['darwin-arm64', 'darwin-x86_64'],
        'linux' => const ['linux-x86_64'],
        _ => const <String>[],
      }.cast<String?>().firstWhere(
        (tag) => Directory.fromUri(
          ndk.uri.resolve('toolchains/llvm/prebuilt/$tag/'),
        ).existsSync(),
        orElse: () => null,
      );
  if (hostTag == null) {
    throw BuildError(
      message:
          'flark_parse: no prebuilt LLVM toolchain under ${ndk.path}/toolchains/llvm/prebuilt.',
    );
  }
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
  for (final key in [
    'ANDROID_NDK_HOME',
    'ANDROID_NDK',
    'ANDROID_NDK_ROOT',
    'ANDROID_NDK_LATEST_HOME',
  ]) {
    final path = Platform.environment[key];
    if (path != null && path.isNotEmpty && Directory(path).existsSync()) {
      return Directory(path);
    }
  }
  final home = Platform.environment['ANDROID_HOME'];
  if (home == null || home.isEmpty) return null;
  final root = Directory.fromUri(Directory(home).uri.resolve('ndk/'));
  if (!root.existsSync()) return null;
  final ndks = root.listSync().whereType<Directory>().toList()
    ..sort((a, b) => a.path.compareTo(b.path));
  return ndks.isEmpty ? null : ndks.last;
}
