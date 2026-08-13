enum FlarkCertification { pendingNeutral, currentCertified, mixedCurrent }

enum FlarkHeadingStyle { atx, setext }

enum FlarkListMarkerStyle {
  hyphen,
  plus,
  asterisk,
  orderedPeriod,
  orderedParenthesis,
}

enum FlarkCodeBlockStyle { indented, fencedBacktick, fencedTilde }

enum FlarkViewportRowEditCapability {
  contiguous,
  projectedReserved,
  unavailable,
}

enum FlarkViewportRowContinuityPolicy { none, plainTextEdit }

enum FlarkInlineFactKind {
  emphasis,
  strong,
  code,
  strikethrough,
  autolinkUri,
  autolinkEmail,
  backslashEscape,
  hardLineBreak,
  replacement,
  directLink,
  directImage,
  referenceLink,
  referenceImage,
  tableCell,
}

enum FlarkInlineContinuityPolicy { none, plainTextContent }

const _inlineFactContinuityPlainText = 1 << 7;

enum FlarkTableAlignment { none, left, center, right }

final class FlarkTableCellPresentation {
  const FlarkTableCellPresentation({
    required this.alignment,
    required this.header,
    required this.autocompleted,
    required this.sourceBytes,
    required this.sourceUtf16,
    required this.contentBytes,
    required this.contentUtf16,
  });

  final FlarkTableAlignment alignment;
  final bool header;
  final bool autocompleted;
  final FlarkSourceRange sourceBytes;
  final FlarkSourceRange sourceUtf16;
  final FlarkSourceRange contentBytes;
  final FlarkSourceRange contentUtf16;

  Map<String, Object?> toMessage() => {
    'alignment': alignment.index,
    'header': header,
    'autocompleted': autocompleted,
    'sourceBytes': sourceBytes.toMessage(),
    'sourceUtf16': sourceUtf16.toMessage(),
    'contentBytes': contentBytes.toMessage(),
    'contentUtf16': contentUtf16.toMessage(),
  };

  static FlarkTableCellPresentation fromMessage(
    Map<Object?, Object?> message,
  ) => FlarkTableCellPresentation(
    alignment: FlarkTableAlignment.values[message['alignment']! as int],
    header: message['header']! as bool,
    autocompleted: message['autocompleted']! as bool,
    sourceBytes: FlarkSourceRange.fromMessage(
      message['sourceBytes']! as Map<Object?, Object?>,
    ),
    sourceUtf16: FlarkSourceRange.fromMessage(
      message['sourceUtf16']! as Map<Object?, Object?>,
    ),
    contentBytes: FlarkSourceRange.fromMessage(
      message['contentBytes']! as Map<Object?, Object?>,
    ),
    contentUtf16: FlarkSourceRange.fromMessage(
      message['contentUtf16']! as Map<Object?, Object?>,
    ),
  );
}

final class FlarkTablePresentation {
  const FlarkTablePresentation({required this.rows});

  final List<List<FlarkTableCellPresentation>> rows;

  int get columnCount => rows.isEmpty ? 0 : rows.first.length;

  Map<String, Object?> toMessage() => {
    'rows': rows
        .map(
          (row) => row.map((cell) => cell.toMessage()).toList(growable: false),
        )
        .toList(growable: false),
  };

  static FlarkTablePresentation fromMessage(Map<Object?, Object?> message) =>
      FlarkTablePresentation(
        rows: (message['rows']! as List<Object?>)
            .map(
              (row) => (row! as List<Object?>)
                  .map(
                    (cell) => FlarkTableCellPresentation.fromMessage(
                      cell! as Map<Object?, Object?>,
                    ),
                  )
                  .toList(growable: false),
            )
            .toList(growable: false),
      );
}

final class FlarkListItemPresentation {
  const FlarkListItemPresentation({
    required this.markerStyle,
    required this.markerValue,
    required this.prefixBytes,
    required this.prefixUtf16,
    required this.nestingDepth,
    required this.markerOffset,
    required this.markerColumn,
    required this.simpleContinuation,
    required this.startsList,
    this.taskChecked,
  });

  final FlarkListMarkerStyle markerStyle;
  final int markerValue;
  final FlarkSourceRange prefixBytes;
  final FlarkSourceRange prefixUtf16;
  final int nestingDepth;
  final int markerOffset;
  final int markerColumn;
  final bool simpleContinuation;
  final bool startsList;
  final bool? taskChecked;

  bool get isOrdered =>
      markerStyle == FlarkListMarkerStyle.orderedPeriod ||
      markerStyle == FlarkListMarkerStyle.orderedParenthesis;

  String get markerText => switch (markerStyle) {
    FlarkListMarkerStyle.hyphen => '-',
    FlarkListMarkerStyle.plus => '+',
    FlarkListMarkerStyle.asterisk => '*',
    FlarkListMarkerStyle.orderedPeriod => '$markerValue.',
    FlarkListMarkerStyle.orderedParenthesis => '$markerValue)',
  };

  String get nextMarkerText {
    if (!isOrdered) return markerText;
    final next = markerValue < 999999999 ? markerValue + 1 : markerValue;
    return switch (markerStyle) {
      FlarkListMarkerStyle.orderedPeriod => '$next.',
      FlarkListMarkerStyle.orderedParenthesis => '$next)',
      _ => markerText,
    };
  }

  Map<String, Object?> toMessage() => {
    'markerStyle': markerStyle.index,
    'markerValue': markerValue,
    'prefixBytes': prefixBytes.toMessage(),
    'prefixUtf16': prefixUtf16.toMessage(),
    'nestingDepth': nestingDepth,
    'markerOffset': markerOffset,
    'markerColumn': markerColumn,
    'simpleContinuation': simpleContinuation,
    'startsList': startsList,
    'taskChecked': taskChecked,
  };

  static FlarkListItemPresentation fromMessage(Map<Object?, Object?> message) =>
      FlarkListItemPresentation(
        markerStyle:
            FlarkListMarkerStyle.values[message['markerStyle']! as int],
        markerValue: message['markerValue']! as int,
        prefixBytes: FlarkSourceRange.fromMessage(
          message['prefixBytes']! as Map<Object?, Object?>,
        ),
        prefixUtf16: FlarkSourceRange.fromMessage(
          message['prefixUtf16']! as Map<Object?, Object?>,
        ),
        nestingDepth: message['nestingDepth']! as int,
        markerOffset: message['markerOffset']! as int,
        markerColumn: message['markerColumn']! as int,
        simpleContinuation: message['simpleContinuation']! as bool,
        startsList: message['startsList']! as bool,
        taskChecked: message['taskChecked'] as bool?,
      );
}

final class FlarkBlockQuotePresentation {
  const FlarkBlockQuotePresentation({
    required this.prefixBytes,
    required this.prefixUtf16,
    required this.nestingDepth,
    required this.simpleContinuation,
  });

  final FlarkSourceRange prefixBytes;
  final FlarkSourceRange prefixUtf16;
  final int nestingDepth;
  final bool simpleContinuation;

  Map<String, Object?> toMessage() => {
    'prefixBytes': prefixBytes.toMessage(),
    'prefixUtf16': prefixUtf16.toMessage(),
    'nestingDepth': nestingDepth,
    'simpleContinuation': simpleContinuation,
  };

  static FlarkBlockQuotePresentation fromMessage(
    Map<Object?, Object?> message,
  ) => FlarkBlockQuotePresentation(
    prefixBytes: FlarkSourceRange.fromMessage(
      message['prefixBytes']! as Map<Object?, Object?>,
    ),
    prefixUtf16: FlarkSourceRange.fromMessage(
      message['prefixUtf16']! as Map<Object?, Object?>,
    ),
    nestingDepth: message['nestingDepth']! as int,
    simpleContinuation: message['simpleContinuation']! as bool,
  );
}

final class FlarkCodeBlockPresentation {
  const FlarkCodeBlockPresentation({
    required this.style,
    required this.minimumClosingLength,
    required this.fenceOffset,
    required this.closed,
  });

  final FlarkCodeBlockStyle style;
  final int minimumClosingLength;
  final int fenceOffset;
  final bool closed;

  bool get isFenced => style != FlarkCodeBlockStyle.indented;

  Map<String, Object?> toMessage() => {
    'style': style.index,
    'minimumClosingLength': minimumClosingLength,
    'fenceOffset': fenceOffset,
    'closed': closed,
  };

  static FlarkCodeBlockPresentation fromMessage(
    Map<Object?, Object?> message,
  ) => FlarkCodeBlockPresentation(
    style: FlarkCodeBlockStyle.values[message['style']! as int],
    minimumClosingLength: message['minimumClosingLength']! as int,
    fenceOffset: message['fenceOffset']! as int,
    closed: message['closed']! as bool,
  );
}

final class FlarkSourceRange {
  const FlarkSourceRange(this.start, this.end)
    : assert(start >= 0),
      assert(end >= start);

  final int start;
  final int end;

  int get length => end - start;

  Map<String, Object?> toMessage() => {'start': start, 'end': end};

  static FlarkSourceRange fromMessage(Map<Object?, Object?> message) =>
      FlarkSourceRange(message['start']! as int, message['end']! as int);
}

/// One parser-authored inline semantic in exact document coordinates.
///
/// [sourceBytes] and [sourceUtf16] include Markdown markers. [contentBytes]
/// and [contentUtf16] name the visible content after those markers are hidden.
final class FlarkInlineFact {
  const FlarkInlineFact({
    required this.kind,
    required this.flags,
    required this.sourceBytes,
    required this.sourceUtf16,
    required this.contentBytes,
    required this.contentUtf16,
    this.replacement,
  });

  final FlarkInlineFactKind kind;
  final int flags;
  final FlarkSourceRange sourceBytes;
  final FlarkSourceRange sourceUtf16;
  final FlarkSourceRange contentBytes;
  final FlarkSourceRange contentUtf16;

  FlarkInlineContinuityPolicy get continuityPolicy =>
      flags & _inlineFactContinuityPlainText != 0
      ? FlarkInlineContinuityPolicy.plainTextContent
      : FlarkInlineContinuityPolicy.none;

  /// Parser-cooked visible text replacing [sourceUtf16], when present.
  final String? replacement;

  Map<String, Object?> toMessage() => {
    'kind': kind.index,
    'flags': flags,
    'sourceBytes': sourceBytes.toMessage(),
    'sourceUtf16': sourceUtf16.toMessage(),
    'contentBytes': contentBytes.toMessage(),
    'contentUtf16': contentUtf16.toMessage(),
    'replacement': replacement,
  };

  static FlarkInlineFact fromMessage(Map<Object?, Object?> message) =>
      FlarkInlineFact(
        kind: FlarkInlineFactKind.values[message['kind']! as int],
        flags: message['flags']! as int,
        sourceBytes: FlarkSourceRange.fromMessage(
          message['sourceBytes']! as Map<Object?, Object?>,
        ),
        sourceUtf16: FlarkSourceRange.fromMessage(
          message['sourceUtf16']! as Map<Object?, Object?>,
        ),
        contentBytes: FlarkSourceRange.fromMessage(
          message['contentBytes']! as Map<Object?, Object?>,
        ),
        contentUtf16: FlarkSourceRange.fromMessage(
          message['contentUtf16']! as Map<Object?, Object?>,
        ),
        replacement: message['replacement'] as String?,
      );
}

final class FlarkCertificationRange {
  const FlarkCertificationRange({
    required this.certification,
    required this.sourceBytes,
    required this.sourceUtf16,
  });

  final FlarkCertification certification;
  final FlarkSourceRange sourceBytes;
  final FlarkSourceRange sourceUtf16;

  bool get isCertified => certification == FlarkCertification.currentCertified;

  Map<String, Object?> toMessage() => {
    'certification': certification.index,
    'sourceBytes': sourceBytes.toMessage(),
    'sourceUtf16': sourceUtf16.toMessage(),
  };

  static FlarkCertificationRange fromMessage(Map<Object?, Object?> message) =>
      FlarkCertificationRange(
        certification:
            FlarkCertification.values[message['certification']! as int],
        sourceBytes: FlarkSourceRange.fromMessage(
          message['sourceBytes']! as Map<Object?, Object?>,
        ),
        sourceUtf16: FlarkSourceRange.fromMessage(
          message['sourceUtf16']! as Map<Object?, Object?>,
        ),
      );
}

/// One exact identity-source cut in a parser-certified projected row.
final class FlarkProjectionSegment {
  const FlarkProjectionSegment({
    required this.sourceBytes,
    required this.sourceUtf16,
  });

  final FlarkSourceRange sourceBytes;
  final FlarkSourceRange sourceUtf16;

  Map<String, Object?> toMessage() => {
    'sourceBytes': sourceBytes.toMessage(),
    'sourceUtf16': sourceUtf16.toMessage(),
  };

  static FlarkProjectionSegment fromMessage(Map<Object?, Object?> message) =>
      FlarkProjectionSegment(
        sourceBytes: FlarkSourceRange.fromMessage(
          message['sourceBytes']! as Map<Object?, Object?>,
        ),
        sourceUtf16: FlarkSourceRange.fromMessage(
          message['sourceUtf16']! as Map<Object?, Object?>,
        ),
      );
}

final class FlarkViewportRow {
  const FlarkViewportRow({
    required this.ordinal,
    required this.kind,
    required this.sourceBytes,
    required this.sourceUtf16,
    required this.editableBytes,
    required this.editableUtf16,
    required this.editCapability,
    this.continuityPolicy = FlarkViewportRowContinuityPolicy.none,
    required this.headingLevel,
    required this.headingStyle,
    required this.listItem,
    required this.blockQuote,
    required this.codeBlock,
    required this.thematicBreak,
    this.table,
    required this.pathDepth,
    this.inlineFacts,
    this.projectionSegments,
  });

  final int ordinal;
  final int kind;
  final FlarkSourceRange sourceBytes;
  final FlarkSourceRange sourceUtf16;
  final FlarkSourceRange? editableBytes;
  final FlarkSourceRange? editableUtf16;
  final FlarkViewportRowEditCapability editCapability;
  final FlarkViewportRowContinuityPolicy continuityPolicy;
  final int? headingLevel;
  final FlarkHeadingStyle? headingStyle;
  final FlarkListItemPresentation? listItem;
  final FlarkBlockQuotePresentation? blockQuote;
  final FlarkCodeBlockPresentation? codeBlock;
  final bool thematicBreak;
  final FlarkTablePresentation? table;
  final int pathDepth;

  /// `null` means inline presentation is unavailable and exact source is
  /// required. An empty list authoritatively means no inline semantics.
  final List<FlarkInlineFact>? inlineFacts;

  /// Exact ordered identity cuts for a [FlarkViewportRowEditCapability.projectedReserved]
  /// row. Gaps between cuts are parser-certified hidden container material.
  final List<FlarkProjectionSegment>? projectionSegments;

  Map<String, Object?> toMessage() => {
    'ordinal': ordinal,
    'kind': kind,
    'sourceBytes': sourceBytes.toMessage(),
    'sourceUtf16': sourceUtf16.toMessage(),
    'editableBytes': editableBytes?.toMessage(),
    'editableUtf16': editableUtf16?.toMessage(),
    'editCapability': editCapability.index,
    'continuityPolicy': continuityPolicy.index,
    'headingLevel': headingLevel,
    'headingStyle': headingStyle?.index,
    'listItem': listItem?.toMessage(),
    'blockQuote': blockQuote?.toMessage(),
    'codeBlock': codeBlock?.toMessage(),
    'thematicBreak': thematicBreak,
    'table': table?.toMessage(),
    'pathDepth': pathDepth,
    'inlineFacts': inlineFacts
        ?.map((fact) => fact.toMessage())
        .toList(growable: false),
    'projectionSegments': projectionSegments
        ?.map((segment) => segment.toMessage())
        .toList(growable: false),
  };

  static FlarkViewportRow fromMessage(
    Map<Object?, Object?> message,
  ) => FlarkViewportRow(
    ordinal: message['ordinal']! as int,
    kind: message['kind']! as int,
    sourceBytes: FlarkSourceRange.fromMessage(
      message['sourceBytes']! as Map<Object?, Object?>,
    ),
    sourceUtf16: FlarkSourceRange.fromMessage(
      message['sourceUtf16']! as Map<Object?, Object?>,
    ),
    editableBytes: switch (message['editableBytes']) {
      final Map<Object?, Object?> range => FlarkSourceRange.fromMessage(range),
      _ => null,
    },
    editableUtf16: switch (message['editableUtf16']) {
      final Map<Object?, Object?> range => FlarkSourceRange.fromMessage(range),
      _ => null,
    },
    editCapability: FlarkViewportRowEditCapability
        .values[message['editCapability']! as int],
    continuityPolicy: FlarkViewportRowContinuityPolicy
        .values[(message['continuityPolicy'] as int?) ?? 0],
    headingLevel: message['headingLevel'] as int?,
    headingStyle: switch (message['headingStyle']) {
      final int index => FlarkHeadingStyle.values[index],
      _ => null,
    },
    listItem: switch (message['listItem']) {
      final Map<Object?, Object?> item => FlarkListItemPresentation.fromMessage(
        item,
      ),
      _ => null,
    },
    blockQuote: switch (message['blockQuote']) {
      final Map<Object?, Object?> quote =>
        FlarkBlockQuotePresentation.fromMessage(quote),
      _ => null,
    },
    codeBlock: switch (message['codeBlock']) {
      final Map<Object?, Object?> code =>
        FlarkCodeBlockPresentation.fromMessage(code),
      _ => null,
    },
    thematicBreak: message['thematicBreak']! as bool,
    table: switch (message['table']) {
      final Map<Object?, Object?> table => FlarkTablePresentation.fromMessage(
        table,
      ),
      _ => null,
    },
    pathDepth: message['pathDepth']! as int,
    inlineFacts: switch (message['inlineFacts']) {
      final List<Object?> facts =>
        facts
            .map(
              (fact) =>
                  FlarkInlineFact.fromMessage(fact! as Map<Object?, Object?>),
            )
            .toList(growable: false),
      _ => null,
    },
    projectionSegments: switch (message['projectionSegments']) {
      final List<Object?> segments =>
        segments
            .map(
              (segment) => FlarkProjectionSegment.fromMessage(
                segment! as Map<Object?, Object?>,
              ),
            )
            .toList(growable: false),
      _ => null,
    },
  );
}

final class FlarkViewport {
  const FlarkViewport({
    required this.revision,
    required this.snapshot,
    required this.requestedBytes,
    required this.coveredBytes,
    required this.coveredUtf16,
    required this.certification,
    required this.rows,
    required this.neutralSource,
    required this.continuation,
    this.certificationRanges = const [],
  });

  final int revision;
  final int snapshot;
  final FlarkSourceRange requestedBytes;
  final FlarkSourceRange coveredBytes;
  final FlarkSourceRange coveredUtf16;
  final FlarkCertification certification;
  final List<FlarkViewportRow> rows;
  final String? neutralSource;
  final int continuation;
  final List<FlarkCertificationRange> certificationRanges;

  bool get isCertified => certification == FlarkCertification.currentCertified;

  Map<String, Object?> toMessage() => {
    'revision': revision,
    'snapshot': snapshot,
    'requestedBytes': requestedBytes.toMessage(),
    'coveredBytes': coveredBytes.toMessage(),
    'coveredUtf16': coveredUtf16.toMessage(),
    'certification': certification.index,
    'rows': rows.map((row) => row.toMessage()).toList(growable: false),
    'neutralSource': neutralSource,
    'continuation': continuation,
    'certificationRanges': certificationRanges
        .map((range) => range.toMessage())
        .toList(growable: false),
  };

  static FlarkViewport fromMessage(Map<Object?, Object?> message) =>
      FlarkViewport(
        revision: message['revision']! as int,
        snapshot: message['snapshot']! as int,
        requestedBytes: FlarkSourceRange.fromMessage(
          message['requestedBytes']! as Map<Object?, Object?>,
        ),
        coveredBytes: FlarkSourceRange.fromMessage(
          message['coveredBytes']! as Map<Object?, Object?>,
        ),
        coveredUtf16: FlarkSourceRange.fromMessage(
          message['coveredUtf16']! as Map<Object?, Object?>,
        ),
        certification:
            FlarkCertification.values[message['certification']! as int],
        rows: (message['rows']! as List<Object?>)
            .cast<Map<Object?, Object?>>()
            .map(FlarkViewportRow.fromMessage)
            .toList(growable: false),
        neutralSource: message['neutralSource'] as String?,
        continuation: message['continuation']! as int,
        certificationRanges:
            ((message['certificationRanges'] as List<Object?>?) ?? const [])
                .cast<Map<Object?, Object?>>()
                .map(FlarkCertificationRange.fromMessage)
                .toList(growable: false),
      );
}
