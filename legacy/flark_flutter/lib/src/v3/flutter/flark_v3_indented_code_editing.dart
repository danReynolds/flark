import 'package:flark/flark_adapter.dart';

import 'flark_v3_hidden_line_prefix_editing.dart';

/// Canonical edit policy for one parser-certified indented-code projection.
///
/// This policy never recognizes Markdown. The caller selects it only after the
/// parser has classified the bounded source projection as indented code, and
/// the projection identifies every hidden source prefix. Provisional
/// projections mechanically derived from that certified lease retain this
/// policy so rapid edits do not wait for parser recertification.
final class FlarkV3IndentedCodeEditPolicy
    implements FlarkV3SourceProjectionEditPolicy {
  factory FlarkV3IndentedCodeEditPolicy({
    String canonicalContinuationPrefix = '    ',
    String canonicalLineEnding = '\n',
  }) => FlarkV3IndentedCodeEditPolicy._(
    FlarkV3HiddenLinePrefixEditPolicy(
      canonicalContinuationPrefix: canonicalContinuationPrefix,
      canonicalLineEnding: canonicalLineEnding,
      projectionLabel: 'Indented-code',
    ),
  );

  const FlarkV3IndentedCodeEditPolicy._(this._delegate);

  /// Keeps editor-command configuration bounded independently of document size.
  static const int maximumCanonicalContinuationPrefixUtf16 =
      FlarkV3HiddenLinePrefixEditPolicy.maximumCanonicalContinuationPrefixUtf16;

  final FlarkV3HiddenLinePrefixEditPolicy _delegate;

  String get canonicalContinuationPrefix =>
      _delegate.canonicalContinuationPrefix;
  String get canonicalLineEnding => _delegate.canonicalLineEnding;

  @override
  FlarkV3SourceProjectionEditPlan planEdit(
    FlarkV3SourceProjectionEditRequest request,
  ) => _delegate.planEdit(request);
}

/// Canonical edit policy for one parser-certified block-quote projection.
///
/// Quote markers remain canonical source owned by the parser-authored
/// projection. A displayed Enter inserts one canonical `> ` continuation;
/// this class does not recognize quote syntax from source text.
final class FlarkV3BlockQuoteEditPolicy
    implements FlarkV3SourceProjectionEditPolicy {
  factory FlarkV3BlockQuoteEditPolicy({
    String canonicalContinuationPrefix = '> ',
    String canonicalLineEnding = '\n',
  }) => FlarkV3BlockQuoteEditPolicy._(
    FlarkV3HiddenLinePrefixEditPolicy(
      canonicalContinuationPrefix: canonicalContinuationPrefix,
      canonicalLineEnding: canonicalLineEnding,
      projectionLabel: 'Block-quote',
    ),
  );

  const FlarkV3BlockQuoteEditPolicy._(this._delegate);

  static const int maximumCanonicalContinuationPrefixUtf16 =
      FlarkV3HiddenLinePrefixEditPolicy.maximumCanonicalContinuationPrefixUtf16;

  final FlarkV3HiddenLinePrefixEditPolicy _delegate;

  String get canonicalContinuationPrefix =>
      _delegate.canonicalContinuationPrefix;
  String get canonicalLineEnding => _delegate.canonicalLineEnding;

  @override
  FlarkV3SourceProjectionEditPlan planEdit(
    FlarkV3SourceProjectionEditRequest request,
  ) => _delegate.planEdit(request);
}
