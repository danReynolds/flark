import 'syntax_engine.dart';
import 'syntax_snapshot.dart';

/// Authoritative CommonMark parser backend contract.
///
/// This allows us to swap the current line-scan implementation for a
/// standards-grade backend (for example cmark-gfm) without changing the
/// adapter/controller boundary.
abstract interface class CommonMarkParseBackend {
  const CommonMarkParseBackend();

  String get backendId;

  Future<SyntaxSnapshot> parse(SyntaxParseRequest request);
}
