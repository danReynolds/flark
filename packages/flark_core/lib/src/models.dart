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

enum FlarkLiteralEditClass {
  asciiWordInsertion,
  singleAsciiSpaceInsertion,
  singleAsciiAsteriskInsertion,
}

/// Parser-authored matcher for one bounded projection edit cell.
enum FlarkProjectionEditMatcher {
  /// Any non-noop insertion, deletion, or replacement without CR/LF.
  anyNoCrLfSplice,

  /// A parser-bounded ASCII word splice. Word edits may reach the declared
  /// trigger boundary; one U+0020 insertion requires strict interior guards.
  asciiLiteralSpliceInLiteral,

  /// One UTF-16/ASCII source unit deletion from a parser-authored plain
  /// literal segment. This proof is deliberately one-shot.
  deleteOneAsciiUnitInLiteral,

  /// Exactly one U+0020 insertion at a zero-width parser-authored trigger.
  insertSingleAsciiSpaceAtPoint,

  /// ASCII word or safe ASCII prose-punctuation characters, separated by
  /// single spaces, appended at a parser-authored physical-line end. The carried
  /// proof never admits two consecutive terminal spaces.
  appendAsciiLiteralAtLineEnd,

  /// Exactly one parser-declared Unicode scalar inserted at a zero-width
  /// trigger. Core compares the parameter; it does not assign Markdown
  /// meaning to the scalar. This proof is deliberately one-shot.
  insertExactScalarAtPoint,

  /// One exact parser-declared splice whose result replaces the current block
  /// shell with the typed [FlarkProjectionResultBlockShell].
  exactSpliceReplaceBlockShell,

  /// A parser-declared ASCII prefix sequence that constructs one typed simple
  /// block shell. Core advances only the exact remaining sequence.
  simpleBlockPrefixPlan,
}

enum FlarkProjectionResultBlockKind { plain, atxHeading, blockQuote, listItem }

/// Parser-authored result shell for one bounded pre-edit transition.
final class FlarkProjectionResultBlockShell {
  const FlarkProjectionResultBlockShell({
    required this.kind,
    required this.prefixUtf16Length,
    this.parameter = 0,
  });

  final FlarkProjectionResultBlockKind kind;
  final int prefixUtf16Length;

  /// Heading level or quote depth. Plain/list shells require zero.
  final int parameter;

  Map<String, Object?> toMessage() => {
    'kind': kind.index,
    'prefixUtf16Length': prefixUtf16Length,
    'parameter': parameter,
  };

  static FlarkProjectionResultBlockShell fromMessage(
    Map<Object?, Object?> message,
  ) => FlarkProjectionResultBlockShell(
    kind: FlarkProjectionResultBlockKind.values[message['kind']! as int],
    prefixUtf16Length: message['prefixUtf16Length']! as int,
    parameter: message['parameter']! as int,
  );
}

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

enum FlarkSemanticTargetKind { link, image }

enum FlarkSemanticTargetSyntax { autolinkUri, autolinkEmail, direct, reference }

final class FlarkSemanticTarget {
  const FlarkSemanticTarget({
    required this.kind,
    required this.syntax,
    required this.sourceBytes,
    required this.sourceUtf16,
    required this.contentBytes,
    required this.contentUtf16,
    required this.destinationSourceBytes,
    required this.destinationSourceUtf16,
    required this.titleSourceBytes,
    required this.titleSourceUtf16,
    required this.destination,
    required this.title,
  });

  final FlarkSemanticTargetKind kind;
  final FlarkSemanticTargetSyntax syntax;
  final FlarkSourceRange sourceBytes;
  final FlarkSourceRange sourceUtf16;
  final FlarkSourceRange contentBytes;
  final FlarkSourceRange contentUtf16;
  final FlarkSourceRange destinationSourceBytes;
  final FlarkSourceRange destinationSourceUtf16;
  final FlarkSourceRange? titleSourceBytes;
  final FlarkSourceRange? titleSourceUtf16;
  final String destination;
  final String? title;

  Map<String, Object?> toMessage() => {
    'kind': kind.index,
    'syntax': syntax.index,
    'sourceBytes': sourceBytes.toMessage(),
    'sourceUtf16': sourceUtf16.toMessage(),
    'contentBytes': contentBytes.toMessage(),
    'contentUtf16': contentUtf16.toMessage(),
    'destinationSourceBytes': destinationSourceBytes.toMessage(),
    'destinationSourceUtf16': destinationSourceUtf16.toMessage(),
    'titleSourceBytes': titleSourceBytes?.toMessage(),
    'titleSourceUtf16': titleSourceUtf16?.toMessage(),
    'destination': destination,
    'title': title,
  };

  static FlarkSemanticTarget fromMessage(
    Map<Object?, Object?> message,
  ) => FlarkSemanticTarget(
    kind: FlarkSemanticTargetKind.values[message['kind']! as int],
    syntax: FlarkSemanticTargetSyntax.values[message['syntax']! as int],
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
    destinationSourceBytes: FlarkSourceRange.fromMessage(
      message['destinationSourceBytes']! as Map<Object?, Object?>,
    ),
    destinationSourceUtf16: FlarkSourceRange.fromMessage(
      message['destinationSourceUtf16']! as Map<Object?, Object?>,
    ),
    titleSourceBytes: switch (message['titleSourceBytes']) {
      final Map<Object?, Object?> range => FlarkSourceRange.fromMessage(range),
      _ => null,
    },
    titleSourceUtf16: switch (message['titleSourceUtf16']) {
      final Map<Object?, Object?> range => FlarkSourceRange.fromMessage(range),
      _ => null,
    },
    destination: message['destination']! as String,
    title: message['title'] as String?,
  );
}

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

/// Parser-authored positional proof for one declared literal edit class.
///
/// The host does not infer Markdown safety from source text. It only checks
/// that an exact edit matches [editClass] and is contained by both ranges.
final class FlarkLiteralSafeEnvelope {
  const FlarkLiteralSafeEnvelope({
    required this.editClass,
    required this.sourceBytes,
    required this.sourceUtf16,
  });

  final FlarkLiteralEditClass editClass;
  final FlarkSourceRange sourceBytes;
  final FlarkSourceRange sourceUtf16;

  Map<String, Object?> toMessage() => {
    'editClass': editClass.index,
    'sourceBytes': sourceBytes.toMessage(),
    'sourceUtf16': sourceUtf16.toMessage(),
  };

  static FlarkLiteralSafeEnvelope fromMessage(Map<Object?, Object?> message) =>
      FlarkLiteralSafeEnvelope(
        editClass: FlarkLiteralEditClass.values[message['editClass']! as int],
        sourceBytes: FlarkSourceRange.fromMessage(
          message['sourceBytes']! as Map<Object?, Object?>,
        ),
        sourceUtf16: FlarkSourceRange.fromMessage(
          message['sourceUtf16']! as Map<Object?, Object?>,
        ),
      );
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

/// One parser-authored edit cell whose affected source closure may be painted
/// exactly while the certified block shell and, when declared, source outside
/// that closure remain projected.
final class FlarkProjectionEditCell {
  const FlarkProjectionEditCell({
    required this.matcher,
    required this.affectedBytes,
    required this.affectedUtf16,
    required this.triggerBytes,
    required this.triggerUtf16,
    required this.retainBlockShell,
    required this.retainOutsideClosure,
    required this.presentClosureExact,
    required this.chainResultCell,
    this.terminalSpaceAvailable = false,
    this.exactScalar,
    this.resultBlockShell,
    this.blockPrefixPlan,
    this.blockPrefixActivationUtf16Length,
  });

  final FlarkProjectionEditMatcher matcher;
  final FlarkSourceRange affectedBytes;
  final FlarkSourceRange affectedUtf16;
  final FlarkSourceRange triggerBytes;
  final FlarkSourceRange triggerUtf16;
  final bool retainBlockShell;
  final bool retainOutsideClosure;
  final bool presentClosureExact;
  final bool chainResultCell;
  final bool terminalSpaceAvailable;
  final int? exactScalar;
  final FlarkProjectionResultBlockShell? resultBlockShell;
  final String? blockPrefixPlan;
  final int? blockPrefixActivationUtf16Length;

  Map<String, Object?> toMessage() => {
    'matcher': matcher.index,
    'affectedBytes': affectedBytes.toMessage(),
    'affectedUtf16': affectedUtf16.toMessage(),
    'triggerBytes': triggerBytes.toMessage(),
    'triggerUtf16': triggerUtf16.toMessage(),
    'retainBlockShell': retainBlockShell,
    'retainOutsideClosure': retainOutsideClosure,
    'presentClosureExact': presentClosureExact,
    'chainResultCell': chainResultCell,
    'terminalSpaceAvailable': terminalSpaceAvailable,
    'exactScalar': exactScalar,
    'resultBlockShell': resultBlockShell?.toMessage(),
    'blockPrefixPlan': blockPrefixPlan,
    'blockPrefixActivationUtf16Length': blockPrefixActivationUtf16Length,
  };

  static FlarkProjectionEditCell fromMessage(Map<Object?, Object?> message) =>
      FlarkProjectionEditCell(
        matcher: FlarkProjectionEditMatcher.values[message['matcher']! as int],
        affectedBytes: FlarkSourceRange.fromMessage(
          message['affectedBytes']! as Map<Object?, Object?>,
        ),
        affectedUtf16: FlarkSourceRange.fromMessage(
          message['affectedUtf16']! as Map<Object?, Object?>,
        ),
        triggerBytes: FlarkSourceRange.fromMessage(
          message['triggerBytes']! as Map<Object?, Object?>,
        ),
        triggerUtf16: FlarkSourceRange.fromMessage(
          message['triggerUtf16']! as Map<Object?, Object?>,
        ),
        retainBlockShell: message['retainBlockShell']! as bool,
        retainOutsideClosure: message['retainOutsideClosure']! as bool,
        presentClosureExact: message['presentClosureExact']! as bool,
        chainResultCell: message['chainResultCell']! as bool,
        terminalSpaceAvailable:
            message['terminalSpaceAvailable'] as bool? ?? false,
        exactScalar: message['exactScalar'] as int?,
        resultBlockShell: switch (message['resultBlockShell']) {
          final Map<Object?, Object?> value =>
            FlarkProjectionResultBlockShell.fromMessage(value),
          _ => null,
        },
        blockPrefixPlan: message['blockPrefixPlan'] as String?,
        blockPrefixActivationUtf16Length:
            message['blockPrefixActivationUtf16Length'] as int?,
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
    required this.headingLevel,
    required this.headingStyle,
    required this.listItem,
    required this.blockQuote,
    required this.codeBlock,
    required this.thematicBreak,
    this.table,
    required this.pathDepth,
    this.inlineFacts,
    this.literalSafeEnvelopes = const [],
    this.projectionEditCells = const [],
    this.pendingPresentationPlans = const [],
    this.projectionSegments,
  });

  final int ordinal;
  final int kind;
  final FlarkSourceRange sourceBytes;
  final FlarkSourceRange sourceUtf16;
  final FlarkSourceRange? editableBytes;
  final FlarkSourceRange? editableUtf16;
  final FlarkViewportRowEditCapability editCapability;
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

  /// Parser-authored edit-class/range proofs for retaining presentation
  /// through one exact edit while recertification is pending.
  final List<FlarkLiteralSafeEnvelope> literalSafeEnvelopes;

  /// Parser-authored affected-source closures for bounded optimistic edits.
  final List<FlarkProjectionEditCell> projectionEditCells;

  /// Exact parser-authored edit sequences paired with a clean bounded result
  /// snapshot for every accepted prefix.
  final List<FlarkPendingPresentationPlan> pendingPresentationPlans;

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
    'literalSafeEnvelopes': literalSafeEnvelopes
        .map((envelope) => envelope.toMessage())
        .toList(growable: false),
    'projectionEditCells': projectionEditCells
        .map((cell) => cell.toMessage())
        .toList(growable: false),
    'pendingPresentationPlans': pendingPresentationPlans
        .map((plan) => plan.toMessage())
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
    literalSafeEnvelopes: switch (message['literalSafeEnvelopes']) {
      final List<Object?> envelopes =>
        envelopes
            .map(
              (envelope) => FlarkLiteralSafeEnvelope.fromMessage(
                envelope! as Map<Object?, Object?>,
              ),
            )
            .toList(growable: false),
      _ => const [],
    },
    projectionEditCells: switch (message['projectionEditCells']) {
      final List<Object?> cells =>
        cells
            .map(
              (cell) => FlarkProjectionEditCell.fromMessage(
                cell! as Map<Object?, Object?>,
              ),
            )
            .toList(growable: false),
      _ => const [],
    },
    pendingPresentationPlans: switch (message['pendingPresentationPlans']) {
      final List<Object?> plans =>
        plans
            .map(
              (plan) => FlarkPendingPresentationPlan.fromMessage(
                plan! as Map<Object?, Object?>,
              ),
            )
            .toList(growable: false),
      _ => const [],
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

final class FlarkPendingPresentationPlan {
  FlarkPendingPresentationPlan({
    required this.sequence,
    required this.triggerBytes,
    required this.triggerUtf16,
    required this.affectedBytes,
    required this.affectedUtf16,
    required this.replacedRowCount,
    required List<FlarkPendingPresentationStep> steps,
  }) : steps = List.unmodifiable(steps);

  final String sequence;
  final FlarkSourceRange triggerBytes;
  final FlarkSourceRange triggerUtf16;
  final FlarkSourceRange affectedBytes;
  final FlarkSourceRange affectedUtf16;
  final int replacedRowCount;
  final List<FlarkPendingPresentationStep> steps;

  Map<String, Object?> toMessage() => {
    'sequence': sequence,
    'triggerBytes': triggerBytes.toMessage(),
    'triggerUtf16': triggerUtf16.toMessage(),
    'affectedBytes': affectedBytes.toMessage(),
    'affectedUtf16': affectedUtf16.toMessage(),
    'replacedRowCount': replacedRowCount,
    'steps': steps.map((step) => step.toMessage()).toList(growable: false),
  };

  static FlarkPendingPresentationPlan fromMessage(
    Map<Object?, Object?> message,
  ) => FlarkPendingPresentationPlan(
    sequence: message['sequence']! as String,
    triggerBytes: FlarkSourceRange.fromMessage(
      message['triggerBytes']! as Map<Object?, Object?>,
    ),
    triggerUtf16: FlarkSourceRange.fromMessage(
      message['triggerUtf16']! as Map<Object?, Object?>,
    ),
    affectedBytes: FlarkSourceRange.fromMessage(
      message['affectedBytes']! as Map<Object?, Object?>,
    ),
    affectedUtf16: FlarkSourceRange.fromMessage(
      message['affectedUtf16']! as Map<Object?, Object?>,
    ),
    replacedRowCount: message['replacedRowCount']! as int,
    steps: (message['steps']! as List<Object?>)
        .cast<Map<Object?, Object?>>()
        .map(FlarkPendingPresentationStep.fromMessage)
        .toList(growable: false),
  );
}

final class FlarkPendingPresentationStep {
  FlarkPendingPresentationStep({
    required this.prefixLength,
    required this.affectedBytes,
    required this.affectedUtf16,
    required List<FlarkViewportRow> rows,
  }) : rows = List.unmodifiable(rows);

  final int prefixLength;
  final FlarkSourceRange affectedBytes;
  final FlarkSourceRange affectedUtf16;
  final List<FlarkViewportRow> rows;

  Map<String, Object?> toMessage() => {
    'prefixLength': prefixLength,
    'affectedBytes': affectedBytes.toMessage(),
    'affectedUtf16': affectedUtf16.toMessage(),
    'rows': rows.map((row) => row.toMessage()).toList(growable: false),
  };

  static FlarkPendingPresentationStep fromMessage(
    Map<Object?, Object?> message,
  ) => FlarkPendingPresentationStep(
    prefixLength: message['prefixLength']! as int,
    affectedBytes: FlarkSourceRange.fromMessage(
      message['affectedBytes']! as Map<Object?, Object?>,
    ),
    affectedUtf16: FlarkSourceRange.fromMessage(
      message['affectedUtf16']! as Map<Object?, Object?>,
    ),
    rows: (message['rows']! as List<Object?>)
        .cast<Map<Object?, Object?>>()
        .map(FlarkViewportRow.fromMessage)
        .toList(growable: false),
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
