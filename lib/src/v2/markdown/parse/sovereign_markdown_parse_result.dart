import '../../core/selection/sovereign_selection.dart';
import '../../core/transaction/sovereign_source_range.dart';

enum FlarkMarkdownBlockKind {
  document,
  paragraph,
  heading,
  blockquote,
  list,
  listItem,
  thematicBreak,
  codeBlock,
  htmlBlock,
  table,
  tableRow,
  tableCell,
  unknown,
}

enum FlarkMarkdownInlineKind {
  text,
  emphasis,
  strong,
  inlineCode,
  link,
  image,
  autolink,
  strikethrough,
  htmlInline,
  unknown,
}

enum FlarkMarkdownHiddenRangeKind {
  markdownMarker,
  blockMarker,
  inlineMarker,
  escapeMarker,
  linkDestination,
  linkTitle,
  referenceDefinition,
  rawHtml,
  unknown,
}

enum FlarkMarkdownReplacementRangeKind { htmlEntity, unknown }

enum FlarkMarkdownAmbiguityKind {
  delimiterRun,
  linkReference,
  tableBoundary,
  rawHtml,
  unknown,
}

final class FlarkMarkdownParseResult {
  FlarkMarkdownParseResult({
    required this.schemaVersion,
    required this.revision,
    required this.sourceTextLength,
    required Iterable<FlarkMarkdownBlockNode> blocks,
    required Iterable<FlarkMarkdownInlineToken> inlineTokens,
    Iterable<FlarkMarkdownHiddenRange> hiddenRanges = const [],
    Iterable<FlarkMarkdownReplacementRange> replacementRanges = const [],
    Iterable<FlarkMarkdownAmbiguityZone> ambiguityZones = const [],
    Iterable<FlarkMarkdownDiagnostic> diagnostics = const [],
    Map<String, Object?> extensions = const {},
  }) : blocks = List<FlarkMarkdownBlockNode>.unmodifiable(blocks),
       inlineTokens = List<FlarkMarkdownInlineToken>.unmodifiable(inlineTokens),
       hiddenRanges = List<FlarkMarkdownHiddenRange>.unmodifiable(hiddenRanges),
       replacementRanges = List<FlarkMarkdownReplacementRange>.unmodifiable(
         replacementRanges,
       ),
       ambiguityZones = List<FlarkMarkdownAmbiguityZone>.unmodifiable(
         ambiguityZones,
       ),
       diagnostics = List<FlarkMarkdownDiagnostic>.unmodifiable(diagnostics),
       extensions = Map<String, Object?>.unmodifiable(extensions);

  factory FlarkMarkdownParseResult.fromJson(Map<String, Object?> json) {
    return FlarkMarkdownParseResult(
      schemaVersion: _int(json['schemaVersion']) ?? 0,
      revision: _int(json['revision']) ?? 0,
      sourceTextLength: _int(json['sourceTextLength']) ?? 0,
      blocks: _list(json['blocks']).map(FlarkMarkdownBlockNode.fromJson),
      inlineTokens: _list(
        json['inlineTokens'],
      ).map(FlarkMarkdownInlineToken.fromJson),
      hiddenRanges: _list(
        json['hiddenRanges'],
      ).map(FlarkMarkdownHiddenRange.fromJson),
      replacementRanges: _list(
        json['replacementRanges'],
      ).map(FlarkMarkdownReplacementRange.fromJson),
      ambiguityZones: _list(
        json['ambiguityZones'],
      ).map(FlarkMarkdownAmbiguityZone.fromJson),
      diagnostics: _list(
        json['diagnostics'],
      ).map(FlarkMarkdownDiagnostic.fromJson),
      extensions: _unknownFields(json, const {
        'schemaVersion',
        'revision',
        'sourceTextLength',
        'blocks',
        'inlineTokens',
        'hiddenRanges',
        'replacementRanges',
        'ambiguityZones',
        'diagnostics',
      }),
    );
  }

  final int schemaVersion;
  final int revision;
  final int sourceTextLength;
  final List<FlarkMarkdownBlockNode> blocks;
  final List<FlarkMarkdownInlineToken> inlineTokens;
  final List<FlarkMarkdownHiddenRange> hiddenRanges;
  final List<FlarkMarkdownReplacementRange> replacementRanges;
  final List<FlarkMarkdownAmbiguityZone> ambiguityZones;
  final List<FlarkMarkdownDiagnostic> diagnostics;
  final Map<String, Object?> extensions;
}

final class FlarkMarkdownBlockNode {
  FlarkMarkdownBlockNode({
    required this.kind,
    required this.type,
    required this.sourceRange,
    Map<String, Object?> attributes = const {},
    Iterable<FlarkMarkdownBlockNode> children = const [],
    Map<String, Object?> extensions = const {},
  }) : attributes = Map<String, Object?>.unmodifiable(attributes),
       children = List<FlarkMarkdownBlockNode>.unmodifiable(children),
       extensions = Map<String, Object?>.unmodifiable(extensions);

  factory FlarkMarkdownBlockNode.fromJson(Map<String, Object?> json) {
    final type = json['type'] as String? ?? 'unknown';
    return FlarkMarkdownBlockNode(
      kind: _blockKind(type),
      type: type,
      sourceRange: _sourceRange(json['sourceRange']),
      attributes: _map(json['attributes']),
      children: _list(json['children']).map(FlarkMarkdownBlockNode.fromJson),
      extensions: _unknownFields(json, const {
        'type',
        'sourceRange',
        'attributes',
        'children',
      }),
    );
  }

  final FlarkMarkdownBlockKind kind;
  final String type;
  final FlarkSourceRange sourceRange;
  final Map<String, Object?> attributes;
  final List<FlarkMarkdownBlockNode> children;
  final Map<String, Object?> extensions;
}

final class FlarkMarkdownInlineToken {
  FlarkMarkdownInlineToken({
    required this.kind,
    required this.type,
    required this.sourceRange,
    Map<String, Object?> attributes = const {},
    Map<String, Object?> extensions = const {},
  }) : attributes = Map<String, Object?>.unmodifiable(attributes),
       extensions = Map<String, Object?>.unmodifiable(extensions);

  factory FlarkMarkdownInlineToken.fromJson(Map<String, Object?> json) {
    final type = json['type'] as String? ?? 'unknown';
    return FlarkMarkdownInlineToken(
      kind: _inlineKind(type),
      type: type,
      sourceRange: _sourceRange(json['sourceRange']),
      attributes: _map(json['attributes']),
      extensions: _unknownFields(json, const {
        'type',
        'sourceRange',
        'attributes',
      }),
    );
  }

  final FlarkMarkdownInlineKind kind;
  final String type;
  final FlarkSourceRange sourceRange;
  final Map<String, Object?> attributes;
  final Map<String, Object?> extensions;
}

final class FlarkMarkdownHiddenRange {
  FlarkMarkdownHiddenRange({
    required this.kind,
    required this.type,
    required this.sourceRange,
    Map<String, Object?> attributes = const {},
    Map<String, Object?> extensions = const {},
  }) : attributes = Map<String, Object?>.unmodifiable(attributes),
       extensions = Map<String, Object?>.unmodifiable(extensions);

  factory FlarkMarkdownHiddenRange.fromJson(Map<String, Object?> json) {
    final type = json['type'] as String? ?? 'unknown';
    return FlarkMarkdownHiddenRange(
      kind: _hiddenRangeKind(type),
      type: type,
      sourceRange: _sourceRange(json['sourceRange']),
      attributes: _map(json['attributes']),
      extensions: _unknownFields(json, const {
        'type',
        'sourceRange',
        'attributes',
      }),
    );
  }

  final FlarkMarkdownHiddenRangeKind kind;
  final String type;
  final FlarkSourceRange sourceRange;
  final Map<String, Object?> attributes;
  final Map<String, Object?> extensions;
}

final class FlarkMarkdownReplacementRange {
  FlarkMarkdownReplacementRange({
    required this.kind,
    required this.type,
    required this.sourceRange,
    required this.replacementText,
    Map<String, Object?> attributes = const {},
    Map<String, Object?> extensions = const {},
  }) : attributes = Map<String, Object?>.unmodifiable(attributes),
       extensions = Map<String, Object?>.unmodifiable(extensions);

  factory FlarkMarkdownReplacementRange.fromJson(Map<String, Object?> json) {
    final type = json['type'] as String? ?? 'unknown';
    return FlarkMarkdownReplacementRange(
      kind: _replacementRangeKind(type),
      type: type,
      sourceRange: _sourceRange(json['sourceRange']),
      replacementText: json['replacementText'] as String? ?? '',
      attributes: _map(json['attributes']),
      extensions: _unknownFields(json, const {
        'type',
        'sourceRange',
        'replacementText',
        'attributes',
      }),
    );
  }

  final FlarkMarkdownReplacementRangeKind kind;
  final String type;
  final FlarkSourceRange sourceRange;
  final String replacementText;
  final Map<String, Object?> attributes;
  final Map<String, Object?> extensions;
}

final class FlarkMarkdownAmbiguityZone {
  FlarkMarkdownAmbiguityZone({
    required this.kind,
    required this.type,
    required this.sourceRange,
    this.preferredAffinity = FlarkMapAffinity.downstream,
    Map<String, Object?> attributes = const {},
    Map<String, Object?> extensions = const {},
  }) : attributes = Map<String, Object?>.unmodifiable(attributes),
       extensions = Map<String, Object?>.unmodifiable(extensions);

  factory FlarkMarkdownAmbiguityZone.fromJson(Map<String, Object?> json) {
    final type = json['type'] as String? ?? 'unknown';
    return FlarkMarkdownAmbiguityZone(
      kind: _ambiguityKind(type),
      type: type,
      sourceRange: _sourceRange(json['sourceRange']),
      preferredAffinity: _affinity(json['preferredAffinity']),
      attributes: _map(json['attributes']),
      extensions: _unknownFields(json, const {
        'type',
        'sourceRange',
        'preferredAffinity',
        'attributes',
      }),
    );
  }

  final FlarkMarkdownAmbiguityKind kind;
  final String type;
  final FlarkSourceRange sourceRange;
  final FlarkMapAffinity preferredAffinity;
  final Map<String, Object?> attributes;
  final Map<String, Object?> extensions;
}

final class FlarkMarkdownDiagnostic {
  FlarkMarkdownDiagnostic({
    required this.code,
    required this.message,
    this.sourceRange,
    Map<String, Object?> extensions = const {},
  }) : extensions = Map<String, Object?>.unmodifiable(extensions);

  factory FlarkMarkdownDiagnostic.fromJson(Map<String, Object?> json) {
    return FlarkMarkdownDiagnostic(
      code: json['code'] as String? ?? 'unknown',
      message: json['message'] as String? ?? '',
      sourceRange: json['sourceRange'] == null
          ? null
          : _sourceRange(json['sourceRange']),
      extensions: _unknownFields(json, const {
        'code',
        'message',
        'sourceRange',
      }),
    );
  }

  final String code;
  final String message;
  final FlarkSourceRange? sourceRange;
  final Map<String, Object?> extensions;
}

FlarkMarkdownBlockKind _blockKind(String type) {
  return switch (type) {
    'document' => FlarkMarkdownBlockKind.document,
    'paragraph' => FlarkMarkdownBlockKind.paragraph,
    'heading' => FlarkMarkdownBlockKind.heading,
    'blockquote' => FlarkMarkdownBlockKind.blockquote,
    'list' => FlarkMarkdownBlockKind.list,
    'listItem' => FlarkMarkdownBlockKind.listItem,
    'thematicBreak' => FlarkMarkdownBlockKind.thematicBreak,
    'codeBlock' => FlarkMarkdownBlockKind.codeBlock,
    'htmlBlock' => FlarkMarkdownBlockKind.htmlBlock,
    'table' => FlarkMarkdownBlockKind.table,
    'tableRow' => FlarkMarkdownBlockKind.tableRow,
    'tableCell' => FlarkMarkdownBlockKind.tableCell,
    _ => FlarkMarkdownBlockKind.unknown,
  };
}

FlarkMarkdownInlineKind _inlineKind(String type) {
  return switch (type) {
    'text' => FlarkMarkdownInlineKind.text,
    'emphasis' => FlarkMarkdownInlineKind.emphasis,
    'strong' => FlarkMarkdownInlineKind.strong,
    'inlineCode' => FlarkMarkdownInlineKind.inlineCode,
    'link' => FlarkMarkdownInlineKind.link,
    'image' => FlarkMarkdownInlineKind.image,
    'autolink' => FlarkMarkdownInlineKind.autolink,
    'strikethrough' => FlarkMarkdownInlineKind.strikethrough,
    'htmlInline' => FlarkMarkdownInlineKind.htmlInline,
    _ => FlarkMarkdownInlineKind.unknown,
  };
}

FlarkMarkdownHiddenRangeKind _hiddenRangeKind(String type) {
  return switch (type) {
    'markdownMarker' => FlarkMarkdownHiddenRangeKind.markdownMarker,
    'blockMarker' => FlarkMarkdownHiddenRangeKind.blockMarker,
    'inlineMarker' => FlarkMarkdownHiddenRangeKind.inlineMarker,
    'escapeMarker' => FlarkMarkdownHiddenRangeKind.escapeMarker,
    'linkDestination' => FlarkMarkdownHiddenRangeKind.linkDestination,
    'linkTitle' => FlarkMarkdownHiddenRangeKind.linkTitle,
    'referenceDefinition' => FlarkMarkdownHiddenRangeKind.referenceDefinition,
    'rawHtml' => FlarkMarkdownHiddenRangeKind.rawHtml,
    _ => FlarkMarkdownHiddenRangeKind.unknown,
  };
}

FlarkMarkdownReplacementRangeKind _replacementRangeKind(String type) {
  return switch (type) {
    'htmlEntity' => FlarkMarkdownReplacementRangeKind.htmlEntity,
    _ => FlarkMarkdownReplacementRangeKind.unknown,
  };
}

FlarkMarkdownAmbiguityKind _ambiguityKind(String type) {
  return switch (type) {
    'delimiterRun' => FlarkMarkdownAmbiguityKind.delimiterRun,
    'linkReference' => FlarkMarkdownAmbiguityKind.linkReference,
    'tableBoundary' => FlarkMarkdownAmbiguityKind.tableBoundary,
    'rawHtml' => FlarkMarkdownAmbiguityKind.rawHtml,
    _ => FlarkMarkdownAmbiguityKind.unknown,
  };
}

FlarkMapAffinity _affinity(Object? value) {
  return switch (value) {
    'upstream' => FlarkMapAffinity.upstream,
    'downstream' => FlarkMapAffinity.downstream,
    _ => FlarkMapAffinity.downstream,
  };
}

FlarkSourceRange _sourceRange(Object? value) {
  final map = _map(value);
  return FlarkSourceRange(_int(map['start']) ?? 0, _int(map['end']) ?? 0);
}

List<Map<String, Object?>> _list(Object? value) {
  if (value is! List) return const [];
  return value
      .whereType<Map>()
      .map((item) {
        return item.cast<String, Object?>();
      })
      .toList(growable: false);
}

Map<String, Object?> _map(Object? value) {
  if (value is! Map) return const {};
  return value.cast<String, Object?>();
}

int? _int(Object? value) {
  if (value is int) return value;
  if (value is num) return value.toInt();
  return null;
}

Map<String, Object?> _unknownFields(
  Map<String, Object?> json,
  Set<String> knownKeys,
) {
  return {
    for (final entry in json.entries)
      if (!knownKeys.contains(entry.key)) entry.key: entry.value,
  };
}
