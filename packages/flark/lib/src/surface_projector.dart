import 'dart:math' as math;

import 'editor_text.dart';
import 'models.dart';
import 'optimistic_range_map.dart';
import 'pending_presentation.dart';
import 'presentation.dart';
import 'surface_projection.dart';

/// Immutable inputs for constructing one controller surface publication.
///
/// Projection has no document handle, timers, queues, or mutation callbacks.
/// Its result is therefore determined entirely by this captured state.
final class FlarkSurfaceProjector {
  FlarkSurfaceProjector({
    required this.pendingPresentation,
    required this.visibleUtf16Start,
    required this.visibleSource,
    required this.inputGlobalUtf16Start,
    required this.inputValue,
    required this.activeOrdinal,
    required this.selectionBaseUtf16,
    required this.selectionExtentUtf16,
    required this.crossRowSelection,
    required this.semanticViewportCurrent,
    required this.certificationRevisionCurrent,
    required List<FlarkCertificationRange> certificationRanges,
    required FlarkOptimisticRangeMap optimisticRanges,
  }) : certificationRanges = List.unmodifiable(certificationRanges),
       optimisticRanges = FlarkOptimisticRangeMap.snapshot(optimisticRanges);

  final FlarkPendingPresentationSnapshot pendingPresentation;
  final int visibleUtf16Start;
  final String visibleSource;
  final int inputGlobalUtf16Start;
  final FlarkEditorInputValue inputValue;
  final int? activeOrdinal;
  final int selectionBaseUtf16;
  final int selectionExtentUtf16;
  final bool crossRowSelection;
  final bool semanticViewportCurrent;
  final bool certificationRevisionCurrent;
  final List<FlarkCertificationRange> certificationRanges;
  final FlarkOptimisticRangeMap optimisticRanges;

  FlarkSurfaceRow surfaceRow(
    FlarkViewportRow row, {
    bool includeEditingState = true,
  }) {
    final structurals = _structuralSurfacesFor(row.ordinal);
    if (structurals.isNotEmpty) {
      final structural = structurals.firstWhere(
        (candidate) =>
            candidate.sourceUtf16.start <= selectionExtentUtf16 &&
            selectionExtentUtf16 <= candidate.sourceUtf16.end,
        orElse: () => structurals.first,
      );
      return committedStructuralSurfaceRow(
        structural,
        includeEditingState: includeEditingState,
      );
    }
    final mappedSource = surfaceSourceRange(row);
    final listItem = row.listItem;
    final blockQuote = row.blockQuote;
    final presentationPrefix = listItem?.prefixUtf16 ?? blockQuote?.prefixUtf16;
    final mappedPrefix = presentationPrefix == null
        ? null
        : optimisticRanges.mapRange(presentationPrefix);
    final semanticRange = mappedPrefix == null
        ? mappedSource
        : FlarkSourceRange(mappedPrefix.start, mappedSource.end);
    final baseSemanticRange = FlarkSourceRange(
      presentationPrefix?.start ?? row.sourceUtf16.start,
      row.sourceUtf16.end,
    );
    final rowCertified =
        rowSemanticsCurrent(semanticRange) ||
        optimisticRanges.leavesRangeUnchanged(baseSemanticRange);
    final exactLeadingText = mappedPrefix == null
        ? ''
        : _sliceVisible(mappedPrefix.start, mappedPrefix.end);
    final retainedContainerShell =
        presentationPrefix != null &&
        (optimisticRanges.leavesRangeUnchanged(presentationPrefix) ||
            (row.editableUtf16 != null &&
                optimisticRanges.staysWithin(row.editableUtf16!)));
    final retainedLeadingText = retainedContainerShell
        ? listItem != null
              ? _projectedListPrefix(row.ordinal, listItem)
              : blockQuote != null
              ? _projectedBlockQuotePrefix(blockQuote)
              : exactLeadingText
        : exactLeadingText;
    final continuity = pendingPresentation.dependency;
    final continuityOwnsRow =
        includeEditingState &&
        continuity != null &&
        !crossRowSelection &&
        continuity.rowOrdinal == row.ordinal &&
        activeOrdinal == row.ordinal &&
        semanticRange.start <= selectionExtentUtf16 &&
        selectionExtentUtf16 <= semanticRange.end;
    final caretOwnsRow =
        includeEditingState &&
        !crossRowSelection &&
        semanticRange.start <= selectionExtentUtf16 &&
        selectionExtentUtf16 < semanticRange.end;
    final active =
        includeEditingState &&
        (activeOrdinal == row.ordinal || continuityOwnsRow || caretOwnsRow);
    final selected =
        includeEditingState &&
        crossRowSelection &&
        (_selectionIntersects(semanticRange) || active);
    if (continuityOwnsRow) return _activeContinuitySurface(continuity);
    if (selected && !active && (!rowCertified || row.table != null)) {
      return _exactSelectionSurfaceRow(
        range: semanticRange,
        ordinal: row.ordinal,
      );
    }
    if (active && !rowCertified) {
      final localFocus = inputValue.selection.extentOffset.clamp(
        0,
        inputValue.text.length,
      );
      final localLineStart = localFocus == 0
          ? 0
          : inputValue.text.lastIndexOf('\n', localFocus - 1) + 1;
      final activeLineStart = inputGlobalUtf16Start + localLineStart;
      var paintStart = math.min(
        mappedPrefix?.end ?? mappedSource.start,
        activeLineStart,
      );
      if (mappedPrefix != null) {
        paintStart = math.max(paintStart, mappedPrefix.end);
      }
      final paintInput = FlarkSurfaceProjection.paintInputWindow(
        value: inputValue,
        inputGlobalUtf16Start: inputGlobalUtf16Start,
        sourceStart: paintStart,
        sourceEnd: mappedSource.end,
      );
      return _neutralLineSurfaceRow(
        leadingText: retainedLeadingText,
        globalUtf16Start: paintInput.globalStart,
        text: paintInput.text,
        ordinal: row.ordinal,
        active: active,
        selected: includeEditingState,
      );
    }
    final baseRange = rowCertified
        ? (row.editableUtf16 ?? row.sourceUtf16)
        : row.sourceUtf16;
    final range = optimisticRanges.mapRange(baseRange);
    final leadingText = !rowCertified
        ? exactLeadingText
        : listItem != null
        ? _projectedListPrefix(row.ordinal, listItem)
        : blockQuote != null
        ? _projectedBlockQuotePrefix(blockQuote)
        : '';
    var runs = rowCertified && row.projectionSegments != null
        ? row.projectionSegments!
              .map(
                (segment) => _exactSurfaceRun(
                  optimisticRanges.mapRange(segment.sourceUtf16),
                ),
              )
              .toList(growable: false)
        : rowCertified && row.table != null && row.inlineFacts != null
        ? _projectTableRuns(row.table!, row.inlineFacts!)
        : rowCertified && row.inlineFacts != null
        ? _projectInlineRuns(range, row.inlineFacts!)
        : [_exactSurfaceRun(range)];
    if (active) {
      final trailing = exactTrailingWhitespaceRange(row, selectionExtentUtf16);
      if (trailing != null &&
          (runs.isEmpty || runs.last.sourceUtf16End <= trailing.start)) {
        runs = List.unmodifiable([...runs, _exactSurfaceRun(trailing)]);
      }
    }
    final text = runs.map((run) => run.text).join();
    return FlarkSurfaceRow(
      leadingText: leadingText,
      text: text,
      globalUtf16Start: range.start,
      kind: rowCertified ? row.kind : 0,
      headingLevel: rowCertified ? row.headingLevel : null,
      blockQuoteDepth: rowCertified ? blockQuote?.nestingDepth : null,
      codeBlock: rowCertified ? row.codeBlock : null,
      thematicBreak: rowCertified && row.thematicBreak,
      listItem: rowCertified && listItem != null,
      ordinal: row.ordinal,
      active: active,
      selection: active || selected
          ? _projectedSelection(runs, text.length)
          : null,
      runs: runs,
    );
  }

  List<FlarkSurfaceRow> surfaceRowsFor(
    FlarkViewportRow row, {
    bool includeEditingState = true,
  }) {
    final structurals = _paintedStructuralSurfacesFor(row.ordinal);
    if (structurals.isNotEmpty) {
      return List.unmodifiable(
        structurals.map(
          (structural) => committedStructuralSurfaceRow(
            structural,
            includeEditingState: includeEditingState,
          ),
        ),
      );
    }
    final dependency = pendingPresentation.dependency;
    if (includeEditingState &&
        dependency != null &&
        _dependencyPublicationOwnsSelection(dependency)) {
      if (dependency.rowOrdinal == row.ordinal) {
        return _dependencySurfaceRows(dependency);
      }
      if (dependency.replacedRowOrdinals.contains(row.ordinal)) return const [];
    }
    return [surfaceRow(row, includeEditingState: includeEditingState)];
  }

  List<FlarkCoreCommittedPresentationSurfaceV1> _structuralSurfacesFor(
    int ordinal,
  ) => pendingPresentation.structuralSurfaces
      .map((state) => state.surface)
      .where((surface) => surface.rowOrdinal == ordinal)
      .toList(growable: false);

  List<FlarkCoreCommittedPresentationSurfaceV1> _paintedStructuralSurfacesFor(
    int ordinal,
  ) {
    final surfaces = _structuralSurfacesFor(ordinal);
    return List.unmodifiable([
      for (var index = 0; index < surfaces.length; index += 1)
        if (!_isConsumedStructuralBlockSeparator(surfaces, index))
          surfaces[index],
    ]);
  }

  bool _isConsumedStructuralBlockSeparator(
    List<FlarkCoreCommittedPresentationSurfaceV1> surfaces,
    int index,
  ) {
    final surface = surfaces[index];
    if (surface.role !=
            FlarkCoreCommittedPresentationSurfaceRole.blockSeparator ||
        index + 1 >= surfaces.length) {
      return false;
    }
    final successor = surfaces[index + 1];
    if (successor.role !=
        FlarkCoreCommittedPresentationSurfaceRole.editableSuccessor) {
      return false;
    }
    return '${successor.presentation.leadingText}${successor.presentation.text}'
        .trim()
        .isNotEmpty;
  }

  FlarkSourceRange surfaceSourceRange(FlarkViewportRow row) {
    final structurals = _structuralSurfacesFor(row.ordinal);
    if (structurals.isNotEmpty) {
      return FlarkSourceRange(
        structurals.first.sourceUtf16.start,
        structurals.last.sourceUtf16.end,
      );
    }
    final mapped = mappedExactRowRange(row);
    final continuity = pendingPresentation.dependency;
    var currentMapped =
        continuity?.rowOrdinal == row.ordinal && continuity!.removesOwnerRow
        ? continuity.sourceUtf16
        : continuity?.rowOrdinal == row.ordinal
        ? FlarkSourceRange(
            math.min(mapped.start, continuity!.sourceUtf16.start),
            math.max(mapped.end, continuity.sourceUtf16.end),
          )
        : mapped;
    currentMapped = _includeActiveFallbackLineOwnership(row, currentMapped);
    final split = pendingPresentation.paragraphGap;
    if (split == null ||
        split.rowOrdinal != row.ordinal ||
        split.rowEndUtf16 < currentMapped.start ||
        split.rowEndUtf16 > currentMapped.end) {
      return currentMapped;
    }
    return FlarkSourceRange(currentMapped.start, split.rowEndUtf16);
  }

  FlarkSourceRange _includeActiveFallbackLineOwnership(
    FlarkViewportRow row,
    FlarkSourceRange mapped,
  ) {
    if (crossRowSelection || activeOrdinal != row.ordinal) return mapped;
    final prefix = row.listItem?.prefixUtf16 ?? row.blockQuote?.prefixUtf16;
    final mappedPrefix = prefix == null
        ? null
        : optimisticRanges.mapRange(prefix);
    final semanticRange = mappedPrefix == null
        ? mapped
        : FlarkSourceRange(mappedPrefix.start, mapped.end);
    final baseSemanticRange = FlarkSourceRange(
      prefix?.start ?? row.sourceUtf16.start,
      row.sourceUtf16.end,
    );
    final rowCertified =
        rowSemanticsCurrent(semanticRange) ||
        optimisticRanges.leavesRangeUnchanged(baseSemanticRange);
    if (rowCertified || inputValue.text.isEmpty) return mapped;

    final localFocus = inputValue.selection.extentOffset.clamp(
      0,
      inputValue.text.length,
    );
    final localLineStart = localFocus == 0
        ? 0
        : inputValue.text.lastIndexOf('\n', localFocus - 1) + 1;
    final nextLineEnding = inputValue.text.indexOf('\n', localFocus);
    final localLineEnd = nextLineEnding == -1
        ? inputValue.text.length
        : nextLineEnding + 1;
    var lineStart = inputGlobalUtf16Start + localLineStart;
    final lineEnd = inputGlobalUtf16Start + localLineEnd;
    if (mappedPrefix != null) lineStart = math.max(lineStart, mappedPrefix.end);
    if (lineEnd < mapped.start || lineStart > mapped.end) return mapped;
    return FlarkSourceRange(
      math.min(mapped.start, lineStart),
      math.max(mapped.end, lineEnd),
    );
  }

  FlarkSurfaceRow neutralSurfaceRow({
    required int globalUtf16Start,
    required String text,
    required int ordinal,
    bool includeEditingState = true,
  }) => FlarkSurfaceProjection.neutralRow(
    visibleUtf16Start: visibleUtf16Start,
    visibleSource: visibleSource,
    inputGlobalUtf16Start: inputGlobalUtf16Start,
    inputValue: inputValue,
    activeOrdinal: activeOrdinal,
    canonicalSelectionBaseUtf16: selectionBaseUtf16,
    canonicalSelectionExtentUtf16: selectionExtentUtf16,
    crossRowSelection: crossRowSelection,
    globalUtf16Start: globalUtf16Start,
    text: text,
    ordinal: ordinal,
    includeEditingState: includeEditingState,
  );

  FlarkSurfaceRow _neutralLineSurfaceRow({
    String leadingText = '',
    required int globalUtf16Start,
    required String text,
    required int ordinal,
    required bool active,
    required bool selected,
  }) => FlarkSurfaceProjection.neutralLineRow(
    leadingText: leadingText,
    globalUtf16Start: globalUtf16Start,
    text: text,
    ordinal: ordinal,
    active: active,
    selected: selected,
    canonicalSelectionBaseUtf16: selectionBaseUtf16,
    canonicalSelectionExtentUtf16: selectionExtentUtf16,
    inputSelection: inputValue.selection,
  );

  FlarkSurfaceRow _exactSelectionSurfaceRow({
    required FlarkSourceRange range,
    required int ordinal,
  }) {
    final text = _sliceVisible(range.start, range.end);
    return FlarkSurfaceRow(
      leadingText: '',
      text: text,
      globalUtf16Start: range.start,
      kind: 0,
      headingLevel: null,
      blockQuoteDepth: null,
      codeBlock: null,
      thematicBreak: false,
      ordinal: ordinal,
      active: activeOrdinal == ordinal,
      selection: _selectionForRange(range),
      runs: [
        FlarkSurfaceTextRun(
          text: text,
          sourceUtf16Start: range.start,
          sourceUtf16End: range.end,
          sourceExact: true,
          styles: const {},
        ),
      ],
    );
  }

  FlarkTextSelection _projectedSelection(
    List<FlarkSurfaceTextRun> runs,
    int textLength,
  ) => FlarkSurfaceProjection.projectedSelection(
    runs: runs,
    textLength: textLength,
    base: selectionBaseUtf16,
    extent: selectionExtentUtf16,
    inputSelection: inputValue.selection,
  );

  FlarkSurfaceRow committedStructuralSurfaceRow(
    FlarkCoreCommittedPresentationSurfaceV1 structural, {
    required bool includeEditingState,
  }) {
    if (structural.projectionCurrent) {
      final presentation = structural.presentation;
      final runs = presentation.runs
          .map(
            (run) => FlarkSurfaceTextRun(
              text: run.text,
              sourceUtf16Start: run.sourceUtf16Start,
              sourceUtf16End: run.sourceUtf16End,
              sourceExact: run.sourceExact,
              styles: Set.unmodifiable(run.styles.map(surfaceStyleFromCore)),
            ),
          )
          .toList(growable: false);
      final active =
          includeEditingState &&
          structural.sourceUtf16.start <= selectionExtentUtf16 &&
          selectionExtentUtf16 <= structural.sourceUtf16.end;
      final selected =
          includeEditingState &&
          crossRowSelection &&
          _selectionIntersects(structural.sourceUtf16);
      return FlarkSurfaceRow(
        leadingText: presentation.leadingText,
        text: presentation.text,
        globalUtf16Start: presentation.globalUtf16Start,
        kind: presentation.kind,
        headingLevel: presentation.headingLevel,
        blockQuoteDepth: presentation.blockQuoteDepth,
        codeBlock: presentation.codeBlock,
        thematicBreak: presentation.thematicBreak,
        listItem: presentation.listItem,
        ordinal: structural.rowOrdinal,
        active: active,
        selection: active || selected
            ? _projectedSelection(runs, presentation.text.length)
            : null,
        runs: runs,
      );
    }
    final active =
        includeEditingState &&
        structural.sourceUtf16.start <= selectionExtentUtf16 &&
        selectionExtentUtf16 <= structural.sourceUtf16.end;
    final selected =
        includeEditingState &&
        crossRowSelection &&
        _selectionIntersects(structural.sourceUtf16);
    return _neutralLineSurfaceRow(
      globalUtf16Start: structural.sourceUtf16.start,
      text: _sliceVisible(
        structural.sourceUtf16.start,
        structural.sourceUtf16.end,
      ),
      ordinal: structural.rowOrdinal,
      active: active,
      selected: active || selected,
    );
  }

  FlarkSurfaceRow _activeContinuitySurface(
    FlarkPendingDependencyPresentation continuity,
  ) {
    final presentations = continuity.presentations;
    var selected = presentations.first;
    for (var index = 0; index < presentations.length; index += 1) {
      final candidate = presentations[index];
      if (_presentationOwnsOffset(
        candidate.sourceUtf16,
        selectionExtentUtf16,
        isLast: index == presentations.length - 1,
      )) {
        selected = candidate;
        break;
      }
    }
    final presentation = surfacePresentationFromCore(selected);
    return FlarkSurfaceRow(
      leadingText: presentation.leadingText,
      text: presentation.text,
      globalUtf16Start: presentation.globalUtf16Start,
      kind: presentation.kind,
      headingLevel: presentation.headingLevel,
      blockQuoteDepth: presentation.blockQuoteDepth,
      codeBlock: presentation.codeBlock,
      thematicBreak: presentation.thematicBreak,
      listItem: presentation.listItem,
      ordinal: presentation.ordinal,
      active: true,
      selection: _projectedSelection(
        presentation.runs,
        presentation.text.length,
      ),
      runs: presentation.runs,
    );
  }

  bool _dependencyPublicationOwnsSelection(
    FlarkPendingDependencyPresentation dependency,
  ) =>
      !crossRowSelection &&
      activeOrdinal == dependency.rowOrdinal &&
      dependency.sourceUtf16.start <= selectionExtentUtf16 &&
      selectionExtentUtf16 <= dependency.sourceUtf16.end;

  bool _presentationOwnsOffset(
    FlarkSourceRange range,
    int offset, {
    required bool isLast,
  }) =>
      range.start <= offset &&
      (offset < range.end || (isLast && offset == range.end));

  List<FlarkSurfaceRow> _dependencySurfaceRows(
    FlarkPendingDependencyPresentation dependency,
  ) {
    if (dependency.removesOwnerRow) return const [];
    final result = <FlarkSurfaceRow>[];
    for (var index = 0; index < dependency.presentations.length; index += 1) {
      final core = dependency.presentations[index];
      final presentation = surfacePresentationFromCore(core);
      final active = _presentationOwnsOffset(
        core.sourceUtf16,
        selectionExtentUtf16,
        isLast: index == dependency.presentations.length - 1,
      );
      result.add(
        FlarkSurfaceRow(
          leadingText: presentation.leadingText,
          text: presentation.text,
          globalUtf16Start: presentation.globalUtf16Start,
          kind: presentation.kind,
          headingLevel: presentation.headingLevel,
          blockQuoteDepth: presentation.blockQuoteDepth,
          codeBlock: presentation.codeBlock,
          thematicBreak: presentation.thematicBreak,
          listItem: presentation.listItem,
          ordinal: presentation.ordinal,
          active: active,
          selection: active
              ? _projectedSelection(presentation.runs, presentation.text.length)
              : null,
          runs: presentation.runs,
        ),
      );
    }
    return List.unmodifiable(result);
  }

  FlarkSurfaceRow surfacePresentationFromCore(
    FlarkCorePresentationRow presentation,
  ) {
    final runs = presentation.runs
        .map(
          (run) => FlarkSurfaceTextRun(
            text: run.text,
            sourceUtf16Start: run.sourceUtf16Start,
            sourceUtf16End: run.sourceUtf16End,
            sourceExact: run.sourceExact,
            styles: Set.unmodifiable(run.styles.map(surfaceStyleFromCore)),
          ),
        )
        .toList(growable: false);
    return FlarkSurfaceRow(
      leadingText: presentation.leadingText,
      text: presentation.text,
      globalUtf16Start: presentation.globalUtf16Start,
      kind: presentation.kind,
      headingLevel: presentation.headingLevel,
      blockQuoteDepth: presentation.blockQuoteDepth,
      codeBlock: presentation.codeBlock,
      thematicBreak: presentation.thematicBreak,
      listItem: presentation.listItem,
      ordinal: presentation.ordinal,
      active: false,
      selection: null,
      runs: runs,
    );
  }

  FlarkSurfaceTextRun _exactSurfaceRun(FlarkSourceRange range) =>
      FlarkSurfaceTextRun(
        text: _sliceVisible(range.start, range.end),
        sourceUtf16Start: range.start,
        sourceUtf16End: range.end,
        sourceExact: true,
        styles: const {},
      );

  List<FlarkSurfaceTextRun> _projectInlineRuns(
    FlarkSourceRange range,
    List<FlarkInlineFact> facts,
  ) {
    if (facts.isEmpty) return [_exactSurfaceRun(range)];
    final mapped = facts
        .map(
          (fact) => (
            kind: fact.kind,
            source: optimisticRanges.mapRange(fact.sourceUtf16),
            content: optimisticRanges.mapRange(fact.contentUtf16),
            replacement: fact.replacement,
          ),
        )
        .toList(growable: false);
    final boundaries = <int>{range.start, range.end};
    final hidden = <FlarkSourceRange>[];
    for (final fact in mapped) {
      boundaries
        ..add(fact.source.start)
        ..add(fact.content.start)
        ..add(fact.content.end)
        ..add(fact.source.end);
      if (fact.source.start < fact.content.start) {
        hidden.add(FlarkSourceRange(fact.source.start, fact.content.start));
      }
      if (fact.content.end < fact.source.end) {
        hidden.add(FlarkSourceRange(fact.content.end, fact.source.end));
      }
    }
    final ordered = boundaries.toList()..sort();
    final runs = <FlarkSurfaceTextRun>[];
    for (var index = 0; index + 1 < ordered.length; index++) {
      final start = ordered[index];
      final end = ordered[index + 1];
      if (start == end ||
          start < range.start ||
          end > range.end ||
          hidden.any((cut) => start >= cut.start && end <= cut.end)) {
        continue;
      }
      final styles = <FlarkSurfaceInlineStyle>{};
      for (final fact in mapped) {
        if (start >= fact.content.start && end <= fact.content.end) {
          final style = _surfaceStyleFor(fact.kind);
          if (style != null) styles.add(style);
        }
      }
      String? replacement;
      for (final fact in mapped) {
        if (fact.replacement != null &&
            fact.source.start == start &&
            fact.source.end == end) {
          replacement = fact.replacement;
          break;
        }
      }
      final sourceExact = replacement == null;
      final text = replacement ?? _sliceVisible(start, end);
      if (runs.isNotEmpty &&
          sourceExact &&
          runs.last.sourceExact &&
          runs.last.sourceUtf16End == start &&
          _setsEqual(runs.last.styles, styles)) {
        final prior = runs.removeLast();
        runs.add(
          FlarkSurfaceTextRun(
            text: prior.text + text,
            sourceUtf16Start: prior.sourceUtf16Start,
            sourceUtf16End: end,
            sourceExact: true,
            styles: Set.unmodifiable(styles),
          ),
        );
      } else {
        runs.add(
          FlarkSurfaceTextRun(
            text: text,
            sourceUtf16Start: start,
            sourceUtf16End: end,
            sourceExact: sourceExact,
            styles: Set.unmodifiable(styles),
          ),
        );
      }
    }
    return List.unmodifiable(runs);
  }

  List<FlarkSurfaceTextRun> _projectTableRuns(
    FlarkTablePresentation table,
    List<FlarkInlineFact> facts,
  ) {
    final runs = <FlarkSurfaceTextRun>[];
    for (var rowIndex = 0; rowIndex < table.rows.length; rowIndex++) {
      final cells = table.rows[rowIndex];
      for (var column = 0; column < cells.length; column++) {
        final cell = cells[column];
        final content = optimisticRanges.mapRange(cell.contentUtf16);
        final cellFacts = facts
            .where(
              (fact) =>
                  fact.sourceUtf16.start >= cell.contentUtf16.start &&
                  fact.sourceUtf16.end <= cell.contentUtf16.end,
            )
            .toList(growable: false);
        runs.addAll(_projectInlineRuns(content, cellFacts));
        final lastColumn = column + 1 == cells.length;
        final lastRow = rowIndex + 1 == table.rows.length;
        if (!lastColumn) {
          final next = optimisticRanges.mapRange(
            cells[column + 1].contentUtf16,
          );
          runs.add(
            FlarkSurfaceTextRun(
              text: ' │ ',
              sourceUtf16Start: content.end,
              sourceUtf16End: next.start,
              sourceExact: false,
              styles: const {},
            ),
          );
        } else if (!lastRow) {
          final next = optimisticRanges.mapRange(
            table.rows[rowIndex + 1].first.contentUtf16,
          );
          runs.add(
            FlarkSurfaceTextRun(
              text: '\n',
              sourceUtf16Start: content.end,
              sourceUtf16End: next.start,
              sourceExact: false,
              styles: const {},
            ),
          );
        }
      }
    }
    return List.unmodifiable(runs);
  }

  FlarkSourceRange? exactTrailingWhitespaceRange(
    FlarkViewportRow row,
    int globalCaret,
  ) {
    final editable = row.editableUtf16;
    if (editable == null) return null;
    final mappedEditable = optimisticRanges.mapRange(editable);
    final source = surfaceSourceRange(row);
    var contentEnd = source.end;
    if (contentEnd > source.start &&
        _sliceVisible(contentEnd - 1, contentEnd) == '\n') {
      contentEnd -= 1;
      if (contentEnd > source.start &&
          _sliceVisible(contentEnd - 1, contentEnd) == '\r') {
        contentEnd -= 1;
      }
    } else if (contentEnd > source.start &&
        _sliceVisible(contentEnd - 1, contentEnd) == '\r') {
      contentEnd -= 1;
    }
    if (globalCaret <= mappedEditable.end || globalCaret > contentEnd) {
      return null;
    }
    final trailing = _sliceVisible(mappedEditable.end, contentEnd);
    if (trailing.isEmpty ||
        !trailing.codeUnits.every((unit) => unit == 0x20 || unit == 0x09)) {
      return null;
    }
    return FlarkSourceRange(mappedEditable.end, contentEnd);
  }

  FlarkSourceRange mappedExactRowRange(FlarkViewportRow row) =>
      optimisticRanges.mapRange(_exactRowRange(row));

  FlarkSourceRange _exactRowRange(FlarkViewportRow row) {
    final source = row.sourceUtf16;
    final prefix = row.listItem?.prefixUtf16 ?? row.blockQuote?.prefixUtf16;
    if (prefix == null) return source;
    return FlarkSourceRange(prefix.start, source.end);
  }

  bool rowSemanticsCurrent(FlarkSourceRange mappedSource) {
    if (semanticViewportCurrent) return true;
    if (pendingPresentation.hasPresentationAuthority) return true;
    if (!certificationRevisionCurrent) return false;
    return certificationRanges.any(
      (range) =>
          range.isCertified &&
          range.sourceUtf16.start <= mappedSource.start &&
          mappedSource.end <= range.sourceUtf16.end,
    );
  }

  bool _selectionIntersects(FlarkSourceRange range) =>
      FlarkSurfaceProjection.selectionIntersects(
        range,
        selectionBaseUtf16,
        selectionExtentUtf16,
      );

  FlarkTextSelection _selectionForRange(FlarkSourceRange range) =>
      FlarkTextSelection(
        baseOffset: (selectionBaseUtf16 - range.start).clamp(0, range.length),
        extentOffset: (selectionExtentUtf16 - range.start).clamp(
          0,
          range.length,
        ),
        affinity: inputValue.selection.affinity,
        isDirectional: inputValue.selection.isDirectional,
      );

  String _sliceVisible(int globalStart, int globalEnd) {
    final start = (globalStart - visibleUtf16Start).clamp(
      0,
      visibleSource.length,
    );
    final end = (globalEnd - visibleUtf16Start).clamp(
      start,
      visibleSource.length,
    );
    return visibleSource.substring(start, end);
  }

  String _projectedListPrefix(int rowOrdinal, FlarkListItemPresentation item) {
    final taskChecked =
        pendingPresentation.taskChecks[rowOrdinal] ?? item.taskChecked;
    final marker = switch (taskChecked) {
      null => item.markerText,
      false => '☐',
      true => '☑',
    };
    return '${''.padLeft(item.markerColumn)}$marker ';
  }

  String _projectedBlockQuotePrefix(FlarkBlockQuotePresentation quote) =>
      blockQuotePrefixDepth(quote.nestingDepth);

  static String blockQuotePrefixDepth(int depth) =>
      List<String>.filled(depth, '│ ').join();

  static FlarkSurfaceInlineStyle? _surfaceStyleFor(FlarkInlineFactKind kind) =>
      switch (kind) {
        FlarkInlineFactKind.emphasis => FlarkSurfaceInlineStyle.emphasis,
        FlarkInlineFactKind.strong => FlarkSurfaceInlineStyle.strong,
        FlarkInlineFactKind.code => FlarkSurfaceInlineStyle.code,
        FlarkInlineFactKind.strikethrough =>
          FlarkSurfaceInlineStyle.strikethrough,
        FlarkInlineFactKind.autolinkUri ||
        FlarkInlineFactKind.autolinkEmail ||
        FlarkInlineFactKind.directLink ||
        FlarkInlineFactKind.referenceLink => FlarkSurfaceInlineStyle.link,
        FlarkInlineFactKind.backslashEscape ||
        FlarkInlineFactKind.hardLineBreak ||
        FlarkInlineFactKind.replacement ||
        FlarkInlineFactKind.directImage ||
        FlarkInlineFactKind.referenceImage ||
        FlarkInlineFactKind.tableCell => null,
      };

  static FlarkCorePresentationInlineStyle coreStyleFromSurface(
    FlarkSurfaceInlineStyle style,
  ) => FlarkCorePresentationInlineStyle.values[style.index];

  static FlarkSurfaceInlineStyle surfaceStyleFromCore(
    FlarkCorePresentationInlineStyle style,
  ) => FlarkSurfaceInlineStyle.values[style.index];
}

bool _setsEqual<T>(Set<T> left, Set<T> right) =>
    left.length == right.length && left.containsAll(right);
