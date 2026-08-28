import 'models.dart';
import 'presentation.dart';
import 'projection_continuity.dart';

/// Independently typed parts of one pending-presentation publication.
enum FlarkPendingPresentationPart {
  dependency,
  paragraphGap,
  caretBoundary,
  structuralSurfaces,
  taskChecks,
}

/// One pre-edit dependency proof paired with its exact result presentation.
///
/// [presentations] are framework-neutral. Frontends may add selection and
/// paint geometry, but the source mapping, styles, block shells, and affected-
/// island consequence stay bound to [authority] and [rowOrdinal].
final class FlarkPendingDependencyPresentation {
  FlarkPendingDependencyPresentation({
    required this.rowOrdinal,
    required this.authority,
    required FlarkCorePresentationRow presentation,
    this.removesOwnerRow = false,
  }) : presentations = List.unmodifiable([presentation]),
       replacedRowOrdinals = Set.unmodifiable({rowOrdinal});

  FlarkPendingDependencyPresentation.multi({
    required this.rowOrdinal,
    required this.authority,
    required List<FlarkCorePresentationRow> presentations,
    required Set<int> replacedRowOrdinals,
    this.removesOwnerRow = false,
  }) : assert(presentations.isNotEmpty),
       assert(replacedRowOrdinals.contains(rowOrdinal)),
       presentations = List.unmodifiable(presentations),
       replacedRowOrdinals = Set.unmodifiable(replacedRowOrdinals);

  final int rowOrdinal;
  final FlarkPendingDependencyAuthority authority;
  final List<FlarkCorePresentationRow> presentations;

  /// Cached predecessor ordinals replaced by [presentations]. The owner
  /// ordinal remains the stable publication anchor even when a result merges
  /// or splits the parser's prior row partition.
  final Set<int> replacedRowOrdinals;

  /// The parser proved that the result closure no longer has a rendered
  /// block. [presentations] retains a zero-width source/caret anchor for input
  /// reconciliation, while frontends omit the replaced cached row.
  final bool removesOwnerRow;

  /// Compatibility view for existing one-row dependency authorities.
  FlarkCorePresentationRow get presentation => presentations.single;

  FlarkSourceRange get sourceUtf16 => FlarkSourceRange(
    presentations.first.sourceUtf16.start,
    presentations.last.sourceUtf16.end,
  );

  int get resultRevision => authority.resultRevision;
  FlarkSourceRange get affectedUtf16 => authority.affectedUtf16;
  bool get presentsExactIsland => authority.presentsExactIsland;
}

/// Materializes the current parser-authored plan step from exact visible
/// source. This is protocol projection only: row shells, ranges, facts, and
/// replacements all come from the parser snapshot.
FlarkPendingDependencyPresentation? materializeBoundedPendingPresentationPlan({
  required FlarkBoundedPendingPresentationPlanReceipt authority,
  required int rowOrdinal,
  required String visibleSource,
  required int visibleUtf16Start,
}) {
  final step = authority.step;
  final visibleEnd = visibleUtf16Start + visibleSource.length;
  if (step.rows.isEmpty ||
      step.rows.length > 4 ||
      step.affectedUtf16.start < visibleUtf16Start ||
      step.affectedUtf16.end > visibleEnd) {
    return null;
  }

  String? slice(FlarkSourceRange range) {
    if (range.start < visibleUtf16Start || range.end > visibleEnd) return null;
    return visibleSource.substring(
      range.start - visibleUtf16Start,
      range.end - visibleUtf16Start,
    );
  }

  final presentations = <FlarkCorePresentationRow>[];
  for (var rowIndex = 0; rowIndex < step.rows.length; rowIndex += 1) {
    final row = step.rows[rowIndex];
    if ((row.kind != 5 && row.kind != 7) ||
        row.sourceUtf16.start < step.affectedUtf16.start ||
        row.sourceUtf16.end > step.affectedUtf16.end ||
        row.inlineFacts == null ||
        row.projectionSegments != null ||
        row.pendingPresentationPlans.isNotEmpty) {
      return null;
    }
    final display = row.editableUtf16 ?? row.sourceUtf16;
    if (display.start < row.sourceUtf16.start ||
        display.end > row.sourceUtf16.end ||
        slice(display) == null) {
      return null;
    }
    final runs = _projectPlanInlineRuns(display, row.inlineFacts!, slice);
    if (runs == null) return null;
    presentations.add(
      FlarkCorePresentationRow(
        sourceUtf16: row.sourceUtf16,
        leadingText: '',
        text: runs.map((run) => run.text).join(),
        globalUtf16Start: display.start,
        kind: row.kind,
        headingLevel: row.headingLevel,
        blockQuoteDepth: row.blockQuote?.nestingDepth,
        codeBlock: row.codeBlock,
        thematicBreak: row.thematicBreak,
        listItem: row.listItem != null,
        ordinal: rowOrdinal + rowIndex,
        runs: runs,
      ),
    );
  }
  return FlarkPendingDependencyPresentation.multi(
    rowOrdinal: rowOrdinal,
    authority: authority,
    presentations: presentations,
    replacedRowOrdinals: {
      for (var index = 0; index < authority.plan.replacedRowCount; index += 1)
        rowOrdinal + index,
    },
  );
}

List<FlarkCorePresentationRun>? _projectPlanInlineRuns(
  FlarkSourceRange range,
  List<FlarkInlineFact> facts,
  String? Function(FlarkSourceRange range) slice,
) {
  if (facts.isEmpty) {
    final text = slice(range);
    if (text == null) return null;
    if (text.isEmpty) return const [];
    return [
      FlarkCorePresentationRun(
        text: text,
        sourceUtf16Start: range.start,
        sourceUtf16End: range.end,
        sourceExact: true,
        styles: const {},
      ),
    ];
  }
  if (facts.any(
    (fact) =>
        fact.sourceUtf16.start < range.start ||
        fact.sourceUtf16.end > range.end ||
        fact.contentUtf16.start < fact.sourceUtf16.start ||
        fact.contentUtf16.end > fact.sourceUtf16.end,
  )) {
    return null;
  }
  final boundaries = <int>{range.start, range.end};
  final hidden = <FlarkSourceRange>[];
  for (final fact in facts) {
    boundaries
      ..add(fact.sourceUtf16.start)
      ..add(fact.contentUtf16.start)
      ..add(fact.contentUtf16.end)
      ..add(fact.sourceUtf16.end);
    if (fact.sourceUtf16.start < fact.contentUtf16.start) {
      hidden.add(
        FlarkSourceRange(fact.sourceUtf16.start, fact.contentUtf16.start),
      );
    }
    if (fact.contentUtf16.end < fact.sourceUtf16.end) {
      hidden.add(FlarkSourceRange(fact.contentUtf16.end, fact.sourceUtf16.end));
    }
  }
  final ordered = boundaries.toList()..sort();
  final runs = <FlarkCorePresentationRun>[];
  for (var index = 0; index + 1 < ordered.length; index += 1) {
    final start = ordered[index];
    final end = ordered[index + 1];
    if (start == end ||
        start < range.start ||
        end > range.end ||
        hidden.any((cut) => start >= cut.start && end <= cut.end)) {
      continue;
    }
    final styles = <FlarkCorePresentationInlineStyle>{};
    for (final fact in facts) {
      if (start >= fact.contentUtf16.start && end <= fact.contentUtf16.end) {
        final style = _planStyleFor(fact.kind);
        if (style != null) styles.add(style);
      }
    }
    String? replacement;
    for (final fact in facts) {
      if (fact.replacement != null &&
          fact.sourceUtf16.start == start &&
          fact.sourceUtf16.end == end) {
        replacement = fact.replacement;
        break;
      }
    }
    final text = replacement ?? slice(FlarkSourceRange(start, end));
    if (text == null) return null;
    final sourceExact = replacement == null;
    final immutableStyles = Set<FlarkCorePresentationInlineStyle>.unmodifiable(
      styles,
    );
    if (runs.isNotEmpty &&
        sourceExact &&
        runs.last.sourceExact &&
        runs.last.sourceUtf16End == start &&
        _samePlanStyles(runs.last.styles, immutableStyles)) {
      final prior = runs.removeLast();
      runs.add(
        FlarkCorePresentationRun(
          text: prior.text + text,
          sourceUtf16Start: prior.sourceUtf16Start,
          sourceUtf16End: end,
          sourceExact: true,
          styles: immutableStyles,
        ),
      );
    } else {
      runs.add(
        FlarkCorePresentationRun(
          text: text,
          sourceUtf16Start: start,
          sourceUtf16End: end,
          sourceExact: sourceExact,
          styles: immutableStyles,
        ),
      );
    }
  }
  return List.unmodifiable(runs);
}

FlarkCorePresentationInlineStyle? _planStyleFor(FlarkInlineFactKind kind) =>
    switch (kind) {
      FlarkInlineFactKind.emphasis => FlarkCorePresentationInlineStyle.emphasis,
      FlarkInlineFactKind.strong => FlarkCorePresentationInlineStyle.strong,
      FlarkInlineFactKind.code => FlarkCorePresentationInlineStyle.code,
      FlarkInlineFactKind.strikethrough =>
        FlarkCorePresentationInlineStyle.strikethrough,
      FlarkInlineFactKind.autolinkUri ||
      FlarkInlineFactKind.autolinkEmail ||
      FlarkInlineFactKind.directLink ||
      FlarkInlineFactKind.referenceLink =>
        FlarkCorePresentationInlineStyle.link,
      FlarkInlineFactKind.backslashEscape ||
      FlarkInlineFactKind.hardLineBreak ||
      FlarkInlineFactKind.replacement ||
      FlarkInlineFactKind.directImage ||
      FlarkInlineFactKind.referenceImage ||
      FlarkInlineFactKind.tableCell => null,
    };

bool _samePlanStyles(
  Set<FlarkCorePresentationInlineStyle> left,
  Set<FlarkCorePresentationInlineStyle> right,
) => left.length == right.length && left.every(right.contains);

/// One committed structural surface and any parser-authored successor proof
/// carried by that surface.
final class FlarkPendingStructuralSurface {
  const FlarkPendingStructuralSurface({required this.surface, this.continuity});

  final FlarkCoreCommittedPresentationSurfaceV1 surface;
  final FlarkProjectionEditCellReceipt? continuity;
}

/// Editor-owned caret boundary that remains after its temporary visual gap
/// has been superseded by certified parser rows.
///
/// Markdown has no AST row for this blank interaction island. Keeping the
/// boundary distinct from [FlarkCoreCommittedPresentationGapV1] prevents it
/// from affecting certified layout while still protecting hidden successor
/// syntax from a Delete issued at the shared source offset.
final class FlarkPendingCaretBoundary {
  FlarkPendingCaretBoundary({
    required this.rowOrdinal,
    required this.rowEndUtf16,
    this.authorizedContentUtf16,
    List<FlarkProjectionEditCell> projectionEditCells = const [],
  }) : projectionEditCells = List.unmodifiable(projectionEditCells);

  factory FlarkPendingCaretBoundary.fromGap(
    FlarkCoreCommittedPresentationGapV1 gap, {
    FlarkPendingCaretBoundary? editAuthority,
  }) => FlarkPendingCaretBoundary(
    rowOrdinal: gap.rowOrdinal,
    rowEndUtf16: gap.rowEndUtf16,
    authorizedContentUtf16: editAuthority?.authorizedContentUtf16,
    projectionEditCells:
        editAuthority?.projectionEditCells ?? const <FlarkProjectionEditCell>[],
  );

  final int rowOrdinal;
  final int rowEndUtf16;

  /// Parser-authored first-edit authority retained from the structural
  /// successor after certified rows supersede its temporary visual surface.
  final FlarkSourceRange? authorizedContentUtf16;
  final List<FlarkProjectionEditCell> projectionEditCells;
}

/// The only host-visible pending-presentation authority state.
///
/// Pre-edit dependency proofs and committed structural receipts remain typed
/// inputs with different admission rules. Once admitted, their current
/// presentation, paragraph gap, and semantic-action overlays share this one
/// immutable state and therefore one retirement/supersession lifecycle.
final class FlarkPendingPresentationSnapshot {
  FlarkPendingPresentationSnapshot({
    this.dependency,
    this.paragraphGap,
    this.caretBoundary,
    List<FlarkPendingStructuralSurface> structuralSurfaces = const [],
    Map<int, bool> taskChecks = const {},
  }) : structuralSurfaces = List.unmodifiable(structuralSurfaces),
       taskChecks = Map.unmodifiable(taskChecks);

  const FlarkPendingPresentationSnapshot.empty()
    : dependency = null,
      paragraphGap = null,
      caretBoundary = null,
      structuralSurfaces = const [],
      taskChecks = const {};

  final FlarkPendingDependencyPresentation? dependency;
  final FlarkCoreCommittedPresentationGapV1? paragraphGap;
  final FlarkPendingCaretBoundary? caretBoundary;
  final List<FlarkPendingStructuralSurface> structuralSurfaces;
  final Map<int, bool> taskChecks;

  bool get isEmpty =>
      dependency == null &&
      paragraphGap == null &&
      caretBoundary == null &&
      structuralSurfaces.isEmpty &&
      taskChecks.isEmpty;

  bool get hasPresentationAuthority =>
      dependency != null ||
      structuralSurfaces.isNotEmpty ||
      taskChecks.isNotEmpty;

  FlarkPendingPresentationSnapshot withDependency(
    FlarkPendingDependencyPresentation? value,
  ) => FlarkPendingPresentationSnapshot(
    dependency: value,
    paragraphGap: paragraphGap,
    caretBoundary: caretBoundary,
    structuralSurfaces: structuralSurfaces,
    taskChecks: taskChecks,
  );

  FlarkPendingPresentationSnapshot withParagraphGap(
    FlarkCoreCommittedPresentationGapV1? value,
  ) => FlarkPendingPresentationSnapshot(
    dependency: dependency,
    paragraphGap: value,
    caretBoundary: caretBoundary,
    structuralSurfaces: structuralSurfaces,
    taskChecks: taskChecks,
  );

  FlarkPendingPresentationSnapshot withCaretBoundary(
    FlarkPendingCaretBoundary? value,
  ) => FlarkPendingPresentationSnapshot(
    dependency: dependency,
    paragraphGap: paragraphGap,
    caretBoundary: value,
    structuralSurfaces: structuralSurfaces,
    taskChecks: taskChecks,
  );

  FlarkPendingPresentationSnapshot withStructuralSurfaces(
    List<FlarkPendingStructuralSurface> value,
  ) => FlarkPendingPresentationSnapshot(
    dependency: dependency,
    paragraphGap: paragraphGap,
    caretBoundary: caretBoundary,
    structuralSurfaces: value,
    taskChecks: taskChecks,
  );

  FlarkPendingPresentationSnapshot withTaskCheck(
    int rowOrdinal,
    bool checked,
  ) => FlarkPendingPresentationSnapshot(
    dependency: dependency,
    paragraphGap: paragraphGap,
    caretBoundary: caretBoundary,
    structuralSurfaces: structuralSurfaces,
    taskChecks: {...taskChecks, rowOrdinal: checked},
  );

  /// Retires selected authority through the single snapshot lifecycle.
  FlarkPendingPresentationSnapshot retire(
    Set<FlarkPendingPresentationPart> parts,
  ) {
    if (parts.isEmpty) return this;
    return FlarkPendingPresentationSnapshot(
      dependency: parts.contains(FlarkPendingPresentationPart.dependency)
          ? null
          : dependency,
      paragraphGap: parts.contains(FlarkPendingPresentationPart.paragraphGap)
          ? null
          : paragraphGap,
      caretBoundary: parts.contains(FlarkPendingPresentationPart.caretBoundary)
          ? null
          : caretBoundary,
      structuralSurfaces:
          parts.contains(FlarkPendingPresentationPart.structuralSurfaces)
          ? const []
          : structuralSurfaces,
      taskChecks: parts.contains(FlarkPendingPresentationPart.taskChecks)
          ? const {}
          : taskChecks,
    );
  }

  FlarkPendingPresentationSnapshot clear() =>
      retire(FlarkPendingPresentationPart.values.toSet());
}
