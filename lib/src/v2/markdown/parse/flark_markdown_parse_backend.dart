import 'flark_markdown_parse_result.dart';
import 'flark_markdown_profile.dart';

final class FlarkMarkdownParseRequest {
  const FlarkMarkdownParseRequest({
    required this.revision,
    required this.markdown,
    required this.profile,
  });

  final int revision;
  final String markdown;
  final FlarkMarkdownProfile profile;
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
