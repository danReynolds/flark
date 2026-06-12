import 'dart:io';

import 'package:code_assets/code_assets.dart';
import 'package:hooks/hooks.dart';

const _assetName = 'src/v2/native/native_comrak_ffi.dart';
const _crateRelativePath = 'native/comrak_bridge';
const _libraryBaseName = 'flark_comrak_bridge';

void main(List<String> args) async {
  await build(args, (input, output) async {
    if (!input.config.buildCodeAssets) return;

    final code = input.config.code;
    final packageRoot = input.packageRoot;
    final crateRoot = packageRoot.resolve('$_crateRelativePath/');
    output.dependencies.addAll(_nativeDependencyUris(crateRoot));

    if (code.targetOS == OS.iOS) {
      // The current iOS integration statically links the XCFramework into the
      // app and resolves symbols through DynamicLibrary.process(). Dart's
      // CodeAsset StaticLinking mode is not supported by the SDK yet.
      output.assets.code.add(
        CodeAsset(
          package: input.packageName,
          name: _assetName,
          linkMode: LookupInProcess(),
        ),
      );
      return;
    }

    final plan = _RustBuildPlan.resolve(code);
    if (plan == null) {
      throw BuildError(
        message:
            'Flark native comrak bridge does not support '
            '${code.targetOS}/${code.targetArchitecture}. Supported hook '
            'targets are macOS arm64/x64, Linux arm64/x64, Android '
            'arm/arm64/x64, and iOS process-linked XCFramework builds. See '
            'doc/parser_and_platforms.md ("Build Prerequisites") in the '
            'flark package.',
      );
    }

    final artifact = await _buildRustArtifact(
      plan: plan,
      packageRoot: packageRoot,
      outputDirectory: input.outputDirectory,
    );

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

Iterable<Uri> _nativeDependencyUris(Uri crateRoot) sync* {
  final crateDirectory = Directory.fromUri(crateRoot);
  if (!crateDirectory.existsSync()) return;

  for (final entity in crateDirectory.listSync(recursive: true)) {
    if (entity is! File) continue;
    final path = entity.path;
    if (path.contains(
          '${Platform.pathSeparator}target${Platform.pathSeparator}',
        ) ||
        path.contains(
          '${Platform.pathSeparator}dist${Platform.pathSeparator}',
        )) {
      continue;
    }
    yield entity.uri;
  }
}

Future<Uri> _buildRustArtifact({
  required _RustBuildPlan plan,
  required Uri packageRoot,
  required Uri outputDirectory,
}) async {
  final cratePath = packageRoot.resolve('$_crateRelativePath/Cargo.toml');
  final packageRootPath = packageRoot.toFilePath();
  final cargo = await _cargoCommand();

  if (cargo.usesRustup) {
    await _run(cargo.rustupExecutable!, [
      'target',
      'add',
      plan.triple,
      '--toolchain',
      'stable',
    ], workingDirectory: packageRootPath);
  }

  await _run(
    cargo.executable,
    [
      ...cargo.leadingArgs,
      'build',
      '--manifest-path',
      cratePath.toFilePath(),
      '--release',
      '--target',
      plan.triple,
    ],
    workingDirectory: packageRootPath,
    environment: cargo.buildEnvironment(plan.environment),
  );

  final builtArtifact = packageRoot.resolve(
    '$_crateRelativePath/target/${plan.triple}/release/${plan.libraryFileName}',
  );
  final builtFile = File.fromUri(builtArtifact);
  if (!builtFile.existsSync()) {
    throw BuildError(
      message:
          'Rust build completed but expected artifact was missing: '
          '${builtFile.path}',
    );
  }

  final artifactOutputDirectory = Directory.fromUri(
    outputDirectory.resolve('flark_comrak_bridge/'),
  );
  artifactOutputDirectory.createSync(recursive: true);
  final outputArtifact = artifactOutputDirectory.uri.resolve(
    plan.libraryFileName,
  );
  await builtFile.copy(outputArtifact.toFilePath());
  return outputArtifact;
}

Future<_CargoCommand> _cargoCommand() async {
  final rustup = await _which('rustup');
  if (rustup != null) {
    final cargo = await _rustupWhich(rustup, 'cargo');
    final rustc = await _rustupWhich(rustup, 'rustc');
    if (cargo != null && rustc != null) {
      return _CargoCommand(
        cargo,
        const [],
        rustupExecutable: rustup,
        rustcExecutable: rustc,
        toolchainBinPath: File(rustc).parent.path,
      );
    }
    return _CargoCommand(rustup, const [
      'run',
      'stable',
      'cargo',
    ], rustupExecutable: rustup);
  }
  final cargo = await _which('cargo');
  if (cargo != null) return _CargoCommand(cargo, const []);
  throw BuildError(
    message:
        'Unable to find cargo or rustup on PATH. Flark compiles its native '
        'Comrak bridge from the bundled Rust crate, so building for this '
        'target requires a Rust toolchain (https://rustup.rs). See '
        'doc/parser_and_platforms.md ("Build Prerequisites") in the flark '
        'package.',
  );
}

Future<String?> _which(String executable) async {
  final result = await Process.run('which', [executable]);
  if (result.exitCode != 0) return null;
  final path = (result.stdout as String).trim();
  return path.isEmpty ? null : path;
}

Future<String?> _rustupWhich(String rustup, String executable) async {
  final result = await Process.run(rustup, [
    'which',
    executable,
    '--toolchain',
    'stable',
  ]);
  if (result.exitCode != 0) return null;
  final path = (result.stdout as String).trim();
  return path.isEmpty ? null : path;
}

Future<void> _run(
  String executable,
  List<String> arguments, {
  required String workingDirectory,
  Map<String, String>? environment,
}) async {
  final result = await Process.run(
    executable,
    arguments,
    workingDirectory: workingDirectory,
    environment: environment,
  );
  if (result.exitCode == 0) return;

  throw BuildError(
    message: [
      'Command failed: $executable ${arguments.join(' ')}',
      if ((result.stdout as String).trim().isNotEmpty) result.stdout,
      if ((result.stderr as String).trim().isNotEmpty) result.stderr,
    ].join('\n'),
  );
}

final class _CargoCommand {
  final String executable;
  final List<String> leadingArgs;
  final String? rustupExecutable;
  final String? rustcExecutable;
  final String? toolchainBinPath;

  const _CargoCommand(
    this.executable,
    this.leadingArgs, {
    this.rustupExecutable,
    this.rustcExecutable,
    this.toolchainBinPath,
  });

  bool get usesRustup => rustupExecutable != null;

  Map<String, String>? buildEnvironment(Map<String, String>? base) {
    if (base == null && rustcExecutable == null && toolchainBinPath == null) {
      return null;
    }
    final environment = <String, String>{...?base};
    final rustcPath = rustcExecutable;
    if (rustcPath != null) {
      environment['RUSTC'] = rustcPath;
    }
    final binPath = toolchainBinPath;
    if (binPath != null) {
      final path = Platform.environment['PATH'];
      // The env-list separator (':'/';'), NOT Platform.pathSeparator —
      // that one is the FILE separator ('/' on POSIX) and quietly mangles
      // PATH so child tools (xcrun, cc) stop resolving.
      final envSeparator = Platform.isWindows ? ';' : ':';
      environment['PATH'] = [
        binPath,
        if (path != null && path.isNotEmpty) path,
      ].join(envSeparator);
    }
    return environment;
  }
}

final class _RustBuildPlan {
  final String triple;
  final String libraryFileName;
  final Map<String, String>? environment;

  const _RustBuildPlan({
    required this.triple,
    required this.libraryFileName,
    this.environment,
  });

  static _RustBuildPlan? resolve(CodeConfig code) {
    final os = code.targetOS;
    final architecture = code.targetArchitecture;

    if (os == OS.macOS) {
      final triple = switch (architecture) {
        Architecture.arm64 => 'aarch64-apple-darwin',
        Architecture.x64 => 'x86_64-apple-darwin',
        _ => null,
      };
      if (triple == null) return null;
      return _RustBuildPlan(
        triple: triple,
        libraryFileName: 'lib$_libraryBaseName.dylib',
      );
    }

    if (os == OS.linux) {
      final triple = switch (architecture) {
        Architecture.arm64 => 'aarch64-unknown-linux-gnu',
        Architecture.x64 => 'x86_64-unknown-linux-gnu',
        _ => null,
      };
      if (triple == null) return null;
      return _RustBuildPlan(
        triple: triple,
        libraryFileName: 'lib$_libraryBaseName.so',
      );
    }

    if (os == OS.android) {
      final target = _AndroidTarget.resolve(architecture);
      if (target == null) return null;
      return _RustBuildPlan(
        triple: target.triple,
        libraryFileName: 'lib$_libraryBaseName.so',
        environment: _androidEnvironment(target, code.android.targetNdkApi),
      );
    }

    return null;
  }
}

final class _AndroidTarget {
  final String triple;
  final String linkerPrefix;

  const _AndroidTarget(this.triple, this.linkerPrefix);

  static _AndroidTarget? resolve(Architecture architecture) {
    return switch (architecture) {
      Architecture.arm64 => const _AndroidTarget(
        'aarch64-linux-android',
        'aarch64-linux-android',
      ),
      Architecture.arm => const _AndroidTarget(
        'armv7-linux-androideabi',
        'armv7a-linux-androideabi',
      ),
      Architecture.x64 => const _AndroidTarget(
        'x86_64-linux-android',
        'x86_64-linux-android',
      ),
      _ => null,
    };
  }
}

Map<String, String> _androidEnvironment(_AndroidTarget target, int apiLevel) {
  final ndk = _findAndroidNdk();
  if (ndk == null) {
    throw BuildError(
      message:
          'Android native asset build requires ANDROID_NDK_HOME, '
          'ANDROID_NDK, ANDROID_NDK_ROOT, ANDROID_NDK_LATEST_HOME, or '
          'ANDROID_HOME with an installed NDK.',
    );
  }

  final hostTag = _androidHostTag(ndk);
  if (hostTag == null) {
    throw BuildError(
      message:
          'Unable to find a supported Android NDK prebuilt toolchain '
          'under ${ndk.path}/toolchains/llvm/prebuilt.',
    );
  }

  final toolchain = Directory.fromUri(
    ndk.uri.resolve('toolchains/llvm/prebuilt/$hostTag/bin/'),
  );
  final linker = toolchain.uri
      .resolve('${target.linkerPrefix}$apiLevel-clang')
      .toFilePath();
  final cxx = '$linker++';
  final ar = toolchain.uri.resolve('llvm-ar').toFilePath();
  final tripleUpper = target.triple.toUpperCase().replaceAll('-', '_');
  final tripleSnake = target.triple.replaceAll('-', '_');

  return {
    'CARGO_TARGET_${tripleUpper}_LINKER': linker,
    'CARGO_TARGET_${tripleUpper}_AR': ar,
    'CC_$tripleSnake': linker,
    'CXX_$tripleSnake': cxx,
    'AR_$tripleSnake': ar,
    'TARGET_CC': linker,
    'TARGET_CXX': cxx,
    'TARGET_AR': ar,
    'CC': linker,
    'CXX': cxx,
    'AR': ar,
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
    if (path == null || path.isEmpty) continue;
    final directory = Directory(path);
    if (directory.existsSync()) return directory;
  }

  final androidHome = Platform.environment['ANDROID_HOME'];
  if (androidHome == null || androidHome.isEmpty) return null;
  final ndkRoot = Directory.fromUri(Directory(androidHome).uri.resolve('ndk/'));
  if (!ndkRoot.existsSync()) return null;
  final ndks = ndkRoot.listSync().whereType<Directory>().toList()
    ..sort((a, b) => a.path.compareTo(b.path));
  return ndks.isEmpty ? null : ndks.last;
}

String? _androidHostTag(Directory ndk) {
  final candidates = switch (Platform.operatingSystem) {
    'macos' => const ['darwin-arm64', 'darwin-x86_64'],
    'linux' => const ['linux-x86_64'],
    _ => const <String>[],
  };
  for (final candidate in candidates) {
    final directory = Directory.fromUri(
      ndk.uri.resolve('toolchains/llvm/prebuilt/$candidate/'),
    );
    if (directory.existsSync()) return candidate;
  }
  return null;
}
