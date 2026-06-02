import 'sovereign_markdown_parse_result.dart';
import 'sovereign_markdown_profile.dart';

final class SovereignMarkdownParseRequest {
  const SovereignMarkdownParseRequest({
    required this.revision,
    required this.markdown,
    required this.profile,
  });

  final int revision;
  final String markdown;
  final SovereignMarkdownProfile profile;
}

final class SovereignMarkdownParserCapabilities {
  SovereignMarkdownParserCapabilities({
    required this.parserName,
    required this.schemaVersion,
    required Iterable<SovereignMarkdownProfile> supportedProfiles,
  }) : supportedProfiles = List<SovereignMarkdownProfile>.unmodifiable(
          supportedProfiles,
        );

  final String parserName;
  final int schemaVersion;
  final List<SovereignMarkdownProfile> supportedProfiles;

  bool supports(SovereignMarkdownProfile profile) {
    return supportedProfiles.contains(profile);
  }
}

abstract interface class SovereignMarkdownParseBackend {
  SovereignMarkdownParserCapabilities get capabilities;

  Future<SovereignMarkdownParseResult> parse(
    SovereignMarkdownParseRequest request,
  );
}
