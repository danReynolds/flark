import '../../core/selection/sovereign_selection.dart';
import '../../core/transaction/sovereign_source_range.dart';

enum SovereignMarkdownBlockKind {
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

enum SovereignMarkdownInlineKind {
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

enum SovereignMarkdownHiddenRangeKind {
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

enum SovereignMarkdownReplacementRangeKind {
  htmlEntity,
  unknown,
}

enum SovereignMarkdownAmbiguityKind {
  delimiterRun,
  linkReference,
  tableBoundary,
  rawHtml,
  unknown,
}

final class SovereignMarkdownParseResult {
  SovereignMarkdownParseResult({
    required this.schemaVersion,
    required this.revision,
    required this.sourceTextLength,
    required Iterable<SovereignMarkdownBlockNode> blocks,
    required Iterable<SovereignMarkdownInlineToken> inlineTokens,
    Iterable<SovereignMarkdownHiddenRange> hiddenRanges = const [],
    Iterable<SovereignMarkdownReplacementRange> replacementRanges = const [],
    Iterable<SovereignMarkdownAmbiguityZone> ambiguityZones = const [],
    Iterable<SovereignMarkdownDiagnostic> diagnostics = const [],
    Map<String, Object?> extensions = const {},
  })  : blocks = List<SovereignMarkdownBlockNode>.unmodifiable(blocks),
        inlineTokens = List<SovereignMarkdownInlineToken>.unmodifiable(
          inlineTokens,
        ),
        hiddenRanges = List<SovereignMarkdownHiddenRange>.unmodifiable(
          hiddenRanges,
        ),
        replacementRanges =
            List<SovereignMarkdownReplacementRange>.unmodifiable(
          replacementRanges,
        ),
        ambiguityZones = List<SovereignMarkdownAmbiguityZone>.unmodifiable(
          ambiguityZones,
        ),
        diagnostics = List<SovereignMarkdownDiagnostic>.unmodifiable(
          diagnostics,
        ),
        extensions = Map<String, Object?>.unmodifiable(extensions);

  factory SovereignMarkdownParseResult.fromJson(Map<String, Object?> json) {
    return SovereignMarkdownParseResult(
      schemaVersion: _int(json['schemaVersion']) ?? 0,
      revision: _int(json['revision']) ?? 0,
      sourceTextLength: _int(json['sourceTextLength']) ?? 0,
      blocks: _list(json['blocks']).map(SovereignMarkdownBlockNode.fromJson),
      inlineTokens: _list(
        json['inlineTokens'],
      ).map(SovereignMarkdownInlineToken.fromJson),
      hiddenRanges: _list(
        json['hiddenRanges'],
      ).map(SovereignMarkdownHiddenRange.fromJson),
      replacementRanges: _list(
        json['replacementRanges'],
      ).map(SovereignMarkdownReplacementRange.fromJson),
      ambiguityZones: _list(
        json['ambiguityZones'],
      ).map(SovereignMarkdownAmbiguityZone.fromJson),
      diagnostics: _list(
        json['diagnostics'],
      ).map(SovereignMarkdownDiagnostic.fromJson),
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
  final List<SovereignMarkdownBlockNode> blocks;
  final List<SovereignMarkdownInlineToken> inlineTokens;
  final List<SovereignMarkdownHiddenRange> hiddenRanges;
  final List<SovereignMarkdownReplacementRange> replacementRanges;
  final List<SovereignMarkdownAmbiguityZone> ambiguityZones;
  final List<SovereignMarkdownDiagnostic> diagnostics;
  final Map<String, Object?> extensions;
}

final class SovereignMarkdownBlockNode {
  SovereignMarkdownBlockNode({
    required this.kind,
    required this.type,
    required this.sourceRange,
    Map<String, Object?> attributes = const {},
    Iterable<SovereignMarkdownBlockNode> children = const [],
    Map<String, Object?> extensions = const {},
  })  : attributes = Map<String, Object?>.unmodifiable(attributes),
        children = List<SovereignMarkdownBlockNode>.unmodifiable(children),
        extensions = Map<String, Object?>.unmodifiable(extensions);

  factory SovereignMarkdownBlockNode.fromJson(Map<String, Object?> json) {
    final type = json['type'] as String? ?? 'unknown';
    return SovereignMarkdownBlockNode(
      kind: _blockKind(type),
      type: type,
      sourceRange: _sourceRange(json['sourceRange']),
      attributes: _map(json['attributes']),
      children:
          _list(json['children']).map(SovereignMarkdownBlockNode.fromJson),
      extensions: _unknownFields(json, const {
        'type',
        'sourceRange',
        'attributes',
        'children',
      }),
    );
  }

  final SovereignMarkdownBlockKind kind;
  final String type;
  final SovereignSourceRange sourceRange;
  final Map<String, Object?> attributes;
  final List<SovereignMarkdownBlockNode> children;
  final Map<String, Object?> extensions;
}

final class SovereignMarkdownInlineToken {
  SovereignMarkdownInlineToken({
    required this.kind,
    required this.type,
    required this.sourceRange,
    Map<String, Object?> attributes = const {},
    Map<String, Object?> extensions = const {},
  })  : attributes = Map<String, Object?>.unmodifiable(attributes),
        extensions = Map<String, Object?>.unmodifiable(extensions);

  factory SovereignMarkdownInlineToken.fromJson(Map<String, Object?> json) {
    final type = json['type'] as String? ?? 'unknown';
    return SovereignMarkdownInlineToken(
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

  final SovereignMarkdownInlineKind kind;
  final String type;
  final SovereignSourceRange sourceRange;
  final Map<String, Object?> attributes;
  final Map<String, Object?> extensions;
}

final class SovereignMarkdownHiddenRange {
  SovereignMarkdownHiddenRange({
    required this.kind,
    required this.type,
    required this.sourceRange,
    Map<String, Object?> attributes = const {},
    Map<String, Object?> extensions = const {},
  })  : attributes = Map<String, Object?>.unmodifiable(attributes),
        extensions = Map<String, Object?>.unmodifiable(extensions);

  factory SovereignMarkdownHiddenRange.fromJson(Map<String, Object?> json) {
    final type = json['type'] as String? ?? 'unknown';
    return SovereignMarkdownHiddenRange(
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

  final SovereignMarkdownHiddenRangeKind kind;
  final String type;
  final SovereignSourceRange sourceRange;
  final Map<String, Object?> attributes;
  final Map<String, Object?> extensions;
}

final class SovereignMarkdownReplacementRange {
  SovereignMarkdownReplacementRange({
    required this.kind,
    required this.type,
    required this.sourceRange,
    required this.replacementText,
    Map<String, Object?> attributes = const {},
    Map<String, Object?> extensions = const {},
  })  : attributes = Map<String, Object?>.unmodifiable(attributes),
        extensions = Map<String, Object?>.unmodifiable(extensions);

  factory SovereignMarkdownReplacementRange.fromJson(
    Map<String, Object?> json,
  ) {
    final type = json['type'] as String? ?? 'unknown';
    return SovereignMarkdownReplacementRange(
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

  final SovereignMarkdownReplacementRangeKind kind;
  final String type;
  final SovereignSourceRange sourceRange;
  final String replacementText;
  final Map<String, Object?> attributes;
  final Map<String, Object?> extensions;
}

final class SovereignMarkdownAmbiguityZone {
  SovereignMarkdownAmbiguityZone({
    required this.kind,
    required this.type,
    required this.sourceRange,
    this.preferredAffinity = SovereignMapAffinity.downstream,
    Map<String, Object?> attributes = const {},
    Map<String, Object?> extensions = const {},
  })  : attributes = Map<String, Object?>.unmodifiable(attributes),
        extensions = Map<String, Object?>.unmodifiable(extensions);

  factory SovereignMarkdownAmbiguityZone.fromJson(Map<String, Object?> json) {
    final type = json['type'] as String? ?? 'unknown';
    return SovereignMarkdownAmbiguityZone(
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

  final SovereignMarkdownAmbiguityKind kind;
  final String type;
  final SovereignSourceRange sourceRange;
  final SovereignMapAffinity preferredAffinity;
  final Map<String, Object?> attributes;
  final Map<String, Object?> extensions;
}

final class SovereignMarkdownDiagnostic {
  SovereignMarkdownDiagnostic({
    required this.code,
    required this.message,
    this.sourceRange,
    Map<String, Object?> extensions = const {},
  }) : extensions = Map<String, Object?>.unmodifiable(extensions);

  factory SovereignMarkdownDiagnostic.fromJson(Map<String, Object?> json) {
    return SovereignMarkdownDiagnostic(
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
  final SovereignSourceRange? sourceRange;
  final Map<String, Object?> extensions;
}

SovereignMarkdownBlockKind _blockKind(String type) {
  return switch (type) {
    'document' => SovereignMarkdownBlockKind.document,
    'paragraph' => SovereignMarkdownBlockKind.paragraph,
    'heading' => SovereignMarkdownBlockKind.heading,
    'blockquote' => SovereignMarkdownBlockKind.blockquote,
    'list' => SovereignMarkdownBlockKind.list,
    'listItem' => SovereignMarkdownBlockKind.listItem,
    'thematicBreak' => SovereignMarkdownBlockKind.thematicBreak,
    'codeBlock' => SovereignMarkdownBlockKind.codeBlock,
    'htmlBlock' => SovereignMarkdownBlockKind.htmlBlock,
    'table' => SovereignMarkdownBlockKind.table,
    'tableRow' => SovereignMarkdownBlockKind.tableRow,
    'tableCell' => SovereignMarkdownBlockKind.tableCell,
    _ => SovereignMarkdownBlockKind.unknown,
  };
}

SovereignMarkdownInlineKind _inlineKind(String type) {
  return switch (type) {
    'text' => SovereignMarkdownInlineKind.text,
    'emphasis' => SovereignMarkdownInlineKind.emphasis,
    'strong' => SovereignMarkdownInlineKind.strong,
    'inlineCode' => SovereignMarkdownInlineKind.inlineCode,
    'link' => SovereignMarkdownInlineKind.link,
    'image' => SovereignMarkdownInlineKind.image,
    'autolink' => SovereignMarkdownInlineKind.autolink,
    'strikethrough' => SovereignMarkdownInlineKind.strikethrough,
    'htmlInline' => SovereignMarkdownInlineKind.htmlInline,
    _ => SovereignMarkdownInlineKind.unknown,
  };
}

SovereignMarkdownHiddenRangeKind _hiddenRangeKind(String type) {
  return switch (type) {
    'markdownMarker' => SovereignMarkdownHiddenRangeKind.markdownMarker,
    'blockMarker' => SovereignMarkdownHiddenRangeKind.blockMarker,
    'inlineMarker' => SovereignMarkdownHiddenRangeKind.inlineMarker,
    'escapeMarker' => SovereignMarkdownHiddenRangeKind.escapeMarker,
    'linkDestination' => SovereignMarkdownHiddenRangeKind.linkDestination,
    'linkTitle' => SovereignMarkdownHiddenRangeKind.linkTitle,
    'referenceDefinition' =>
      SovereignMarkdownHiddenRangeKind.referenceDefinition,
    'rawHtml' => SovereignMarkdownHiddenRangeKind.rawHtml,
    _ => SovereignMarkdownHiddenRangeKind.unknown,
  };
}

SovereignMarkdownReplacementRangeKind _replacementRangeKind(String type) {
  return switch (type) {
    'htmlEntity' => SovereignMarkdownReplacementRangeKind.htmlEntity,
    _ => SovereignMarkdownReplacementRangeKind.unknown,
  };
}

SovereignMarkdownAmbiguityKind _ambiguityKind(String type) {
  return switch (type) {
    'delimiterRun' => SovereignMarkdownAmbiguityKind.delimiterRun,
    'linkReference' => SovereignMarkdownAmbiguityKind.linkReference,
    'tableBoundary' => SovereignMarkdownAmbiguityKind.tableBoundary,
    'rawHtml' => SovereignMarkdownAmbiguityKind.rawHtml,
    _ => SovereignMarkdownAmbiguityKind.unknown,
  };
}

SovereignMapAffinity _affinity(Object? value) {
  return switch (value) {
    'upstream' => SovereignMapAffinity.upstream,
    'downstream' => SovereignMapAffinity.downstream,
    _ => SovereignMapAffinity.downstream,
  };
}

SovereignSourceRange _sourceRange(Object? value) {
  final map = _map(value);
  return SovereignSourceRange(
    _int(map['start']) ?? 0,
    _int(map['end']) ?? 0,
  );
}

List<Map<String, Object?>> _list(Object? value) {
  if (value is! List) return const [];
  return value.whereType<Map>().map((item) {
    return item.cast<String, Object?>();
  }).toList(growable: false);
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
