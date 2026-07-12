import 'flark_markdown_parse_result.dart';
import 'flark_markdown_profile.dart';

final class FlarkMarkdownParseRequest {
  const FlarkMarkdownParseRequest({
    required this.revision,
    required this.markdown,
    required this.profile,
    this.maxSyncUtf8Bytes,
  });

  final int revision;
  final String markdown;
  final FlarkMarkdownProfile profile;

  /// Caller-supplied ceiling for a synchronous parse, in UTF-8 bytes.
  ///
  /// A sync-capable backend declines (returns null from `parseSync`) at or
  /// above the ceiling instead of blocking the calling isolate. Null means
  /// the backend's own default. The parse scheduler drives this adaptively —
  /// latency-learned rather than a fixed size cutoff (RFC 022 §6) — so a
  /// fast machine parses large documents authoritatively in-frame while a
  /// slow one degrades to the async path.
  final int? maxSyncUtf8Bytes;
}

final class FlarkMarkdownParserCapabilities {
  FlarkMarkdownParserCapabilities({
    required this.parserName,
    required this.schemaVersion,
    required Iterable<FlarkMarkdownProfile> supportedProfiles,
  }) : supportedProfiles = List<FlarkMarkdownProfile>.unmodifiable(
         supportedProfiles,
       );

  final String parserName;
  final int schemaVersion;
  final List<FlarkMarkdownProfile> supportedProfiles;

  bool supports(FlarkMarkdownProfile profile) {
    return supportedProfiles.contains(profile);
  }
}

abstract interface class FlarkMarkdownParseBackend {
  FlarkMarkdownParserCapabilities get capabilities;

  Future<FlarkMarkdownParseResult> parse(FlarkMarkdownParseRequest request);
}

/// Optional backend capability for synchronous small-document parses.
///
/// The parse scheduler tries this before scheduling an async parse, so a
/// surface's first frame can render an authoritative plan instead of raw
/// source (the visible flash a preview shows for the frame an `await`
/// round-trip takes). Implementations return null whenever a synchronous
/// parse is not possible — input too large for the calling isolate, or a
/// platform whose bridge is inherently async — and callers fall back to
/// [FlarkMarkdownParseBackend.parse].
abstract interface class FlarkSyncCapableParseBackend
    implements FlarkMarkdownParseBackend {
  FlarkMarkdownParseResult? parseSync(FlarkMarkdownParseRequest request);
}
