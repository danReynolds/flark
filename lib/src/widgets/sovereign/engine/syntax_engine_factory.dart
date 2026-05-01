import 'package:flutter/foundation.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/commonmark_parse_backend.dart';
import 'package:sovereign_editor/src/widgets/sovereign/engine/commonmark_syntax_engine_adapter.dart'
    show CommonMarkSyntaxEngineAdapter;
import 'package:sovereign_editor/src/widgets/sovereign/engine/native_comrak_parse_backend.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_engine.dart';

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
