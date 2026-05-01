import 'dart:io';

import 'package:sovereign_editor/src/widgets/sovereign/engine/commonmark_parse_backend.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/native_comrak_parse_backend.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_engine.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_snapshot.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/v1_syntax_engine_adapter.dart';

import 'test_paths.dart';

/// Bootstrap backend used only in test suites.
///
/// It can run native-first or force the V1 adapter as an independent
/// comparator when a test needs non-native cross-checking.
class BootstrapCommonMarkParseBackend implements CommonMarkParseBackend {
  const BootstrapCommonMarkParseBackend({
    this.overrideLibraryPath,
    this.preferNative = true,
  });

  final String? overrideLibraryPath;
  final bool preferNative;

  static CommonMarkParseBackend? _cachedNativeBackend;
  static const CommonMarkParseBackend _fallbackBackend = _V1BootstrapBackend();

  @override
  String get backendId => 'bootstrap_commonmark_v2';

  @override
  Future<SyntaxSnapshot> parse(SyntaxParseRequest request) {
    return _resolveDelegate().parse(request);
  }

  CommonMarkParseBackend _resolveDelegate() {
    if (!preferNative) return _fallbackBackend;

    final override = overrideLibraryPath;
    if (override != null &&
        override.isNotEmpty &&
        File(override).existsSync()) {
      return ComrakCommonMarkParseBackend.withNativeBridge(
        overrideLibraryPath: override,
      );
    }

    final nativePath = sovereignNativeBridgeLibraryPathForPlatform();
    if (nativePath.isNotEmpty && File(nativePath).existsSync()) {
      return _cachedNativeBackend ??=
          ComrakCommonMarkParseBackend.withNativeBridge(
        overrideLibraryPath: nativePath,
      );
    }

    return _fallbackBackend;
  }
}

class _V1BootstrapBackend implements CommonMarkParseBackend {
  const _V1BootstrapBackend();

  static const V1SyntaxEngineAdapter _adapter = V1SyntaxEngineAdapter();

  @override
  String get backendId => 'bootstrap_v1';

  @override
  Future<SyntaxSnapshot> parse(SyntaxParseRequest request) {
    return _adapter.parse(request);
  }
}
