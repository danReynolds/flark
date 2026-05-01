import 'package:flutter/foundation.dart';

import 'commonmark_parse_backend.dart';
import 'commonmark_syntax_engine_adapter.dart'
    show CommonMarkSyntaxEngineAdapter;
import 'native_comrak_parse_backend.dart';
import 'syntax_engine.dart';

class SyntaxEngineFactory {
  const SyntaxEngineFactory._();

  static SyntaxEngine create() {
    return CommonMarkSyntaxEngineAdapter(parseBackend: _createParseBackend());
  }

  @visibleForTesting
  static CommonMarkParseBackend createParseBackendForTesting({
    String? nativeLibraryPathOverride,
  }) {
    return _createParseBackend(
      nativeLibraryPathOverride: nativeLibraryPathOverride,
    );
  }

  static CommonMarkParseBackend _createParseBackend({
    String? nativeLibraryPathOverride,
  }) {
    return ComrakCommonMarkParseBackend.withNativeBridge(
      overrideLibraryPath: nativeLibraryPathOverride,
    );
  }
}
