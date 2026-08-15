import 'dart:convert';
import 'dart:io';

final class FlarkMarkdownFixtureCase {
  const FlarkMarkdownFixtureCase({
    required this.id,
    required this.category,
    required this.markdown,
    this.expectedHtml,
    this.expectedContains = const [],
    this.expectedNotContains = const [],
    this.requiresGfmDifference = false,
  });

  factory FlarkMarkdownFixtureCase.fromJson(Map<String, Object?> json) {
    return FlarkMarkdownFixtureCase(
      id: json['id'] as String? ?? '',
      category: json['category'] as String? ?? 'uncategorized',
      markdown: json['markdown'] as String? ?? '',
      expectedHtml: json['expectedHtml'] as String?,
      expectedContains: _stringList(json['expectedContains']),
      expectedNotContains: _stringList(json['expectedNotContains']),
      requiresGfmDifference: json['requiresGfmDifference'] == true,
    );
  }

  final String id;
  final String category;
  final String markdown;
  final String? expectedHtml;
  final List<String> expectedContains;
  final List<String> expectedNotContains;
  final bool requiresGfmDifference;
}

List<FlarkMarkdownFixtureCase> loadFlarkMarkdownFixtureLane(String path) {
  final decoded = jsonDecode(File(path).readAsStringSync());
  if (decoded is! List) {
    throw FormatException('Expected fixture lane to be a JSON list: $path');
  }
  return decoded
      .whereType<Map>()
      .map((entry) => entry.cast<String, Object?>())
      .map(FlarkMarkdownFixtureCase.fromJson)
      .toList(growable: false);
}

List<String> _stringList(Object? value) {
  if (value is! List) return const [];
  return value.whereType<String>().toList(growable: false);
}
