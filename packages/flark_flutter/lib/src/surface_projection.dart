import 'dart:math' as math;

import 'package:flark/flark.dart';
import 'package:flutter/services.dart';

const _maximumPaintCodeUnits = 2 * 1024;

enum FlarkSurfaceInlineStyle { emphasis, strong, code, strikethrough, link }

final class FlarkSurfaceTextRun {
  FlarkSurfaceTextRun({
    required this.text,
    required this.sourceUtf16Start,
    required this.sourceUtf16End,
    required this.sourceExact,
    required Set<FlarkSurfaceInlineStyle> styles,
  }) : assert(!sourceExact || sourceUtf16End - sourceUtf16Start == text.length),
       styles = Set.unmodifiable(styles);

  final String text;
  final int sourceUtf16Start;
  final int sourceUtf16End;
  final bool sourceExact;
  final Set<FlarkSurfaceInlineStyle> styles;

  int sourceOffsetForTextOffset(
    int offset, {
    TextAffinity affinity = TextAffinity.downstream,
  }) {
    final local = offset.clamp(0, text.length);
    if (sourceExact) return sourceUtf16Start + local;
    if (local == 0) return sourceUtf16Start;
    if (local == text.length) return sourceUtf16End;
    return affinity == TextAffinity.upstream
        ? sourceUtf16Start
        : sourceUtf16End;
  }

  int textOffsetForSourceOffset(
    int offset, {
    TextAffinity affinity = TextAffinity.downstream,
  }) {
    final local = (offset - sourceUtf16Start).clamp(
      0,
      sourceUtf16End - sourceUtf16Start,
    );
    if (sourceExact) return local;
    if (local == 0) return 0;
    if (local == sourceUtf16End - sourceUtf16Start) return text.length;
    return affinity == TextAffinity.upstream ? 0 : text.length;
  }
}

final class FlarkSurfaceRow {
  FlarkSurfaceRow({
    required this.leadingText,
    required this.text,
    required this.globalUtf16Start,
    required this.kind,
    required this.headingLevel,
    required this.blockQuoteDepth,
    required this.codeBlock,
    required this.thematicBreak,
    this.listItem = false,
    required this.ordinal,
    required this.active,
    required this.selection,
    required List<FlarkSurfaceTextRun> runs,
  }) : runs = List.unmodifiable(runs);

  final String leadingText;
  final String text;
  final int globalUtf16Start;
  final int kind;
  final int? headingLevel;
  final int? blockQuoteDepth;
  final FlarkCodeBlockPresentation? codeBlock;
  final bool thematicBreak;
  final bool listItem;
  final int ordinal;
  final bool active;
  final TextSelection? selection;
  final List<FlarkSurfaceTextRun> runs;

  int sourceOffsetForTextOffset(
    int offset, {
    TextAffinity affinity = TextAffinity.downstream,
  }) {
    final local = offset.clamp(0, text.length);
    if (runs.isEmpty) return globalUtf16Start + local;
    var consumed = 0;
    for (var index = 0; index < runs.length; index += 1) {
      final run = runs[index];
      final runEnd = consumed + run.text.length;
      if (local < runEnd) {
        return run.sourceOffsetForTextOffset(
          local - consumed,
          affinity: affinity,
        );
      }
      if (local == runEnd) {
        if (affinity == TextAffinity.downstream && index + 1 < runs.length) {
          return runs[index + 1].sourceUtf16Start;
        }
        return run.sourceUtf16End;
      }
      consumed = runEnd;
    }
    return runs.last.sourceUtf16End;
  }

  int textOffsetForSourceOffset(
    int offset, {
    TextAffinity affinity = TextAffinity.downstream,
  }) {
    if (runs.isEmpty) {
      return (offset - globalUtf16Start).clamp(0, text.length);
    }
    var consumed = 0;
    for (final run in runs) {
      if (offset < run.sourceUtf16Start) return consumed;
      if (offset <= run.sourceUtf16End) {
        return consumed +
            run.textOffsetForSourceOffset(offset, affinity: affinity);
      }
      consumed += run.text.length;
    }
    return text.length;
  }
}

/// One parser row and every render-facing fact captured for the same
/// controller publication. Layout never asks the mutable controller to
/// reconstruct row ownership after this object is created.
final class FlarkSurfacePublicationRow {
  FlarkSurfacePublicationRow({
    required this.row,
    required this.sourceUtf16,
    required List<FlarkSurfaceRow> editingPresentations,
    required List<FlarkSurfaceRow> viewPresentations,
    required this.taskToggleable,
  }) : editingPresentations = List.unmodifiable(editingPresentations),
       viewPresentations = List.unmodifiable(viewPresentations);

  final FlarkViewportRow row;
  final FlarkSourceRange sourceUtf16;
  final List<FlarkSurfaceRow> editingPresentations;
  final List<FlarkSurfaceRow> viewPresentations;
  final bool taskToggleable;

  List<FlarkSurfaceRow> presentations({required bool includeEditingState}) =>
      includeEditingState ? editingPresentations : viewPresentations;
}

/// Immutable render authority sealed at one controller notification.
///
/// Layout, paint, hit testing, and semantics retain this exact object until a
/// newer publication has completed layout. Controller commands remain live,
/// but no visual correctness data is read from mutable controller fields.
final class FlarkSurfacePublication {
  FlarkSurfacePublication({
    required this.sequence,
    required this.interactionGeneration,
    required this.revision,
    required this.sourceGeneration,
    required this.semanticsCurrent,
    required this.viewportPageIndex,
    required this.canPageForward,
    required this.canPageBackward,
    required this.pendingTableNavigationLocked,
    required this.visibleUtf16Start,
    required this.visibleSource,
    required this.canonicalSelectionBaseUtf16,
    required this.canonicalSelectionExtentUtf16,
    required this.inputGlobalUtf16Start,
    required this.inputValue,
    required this.activeOrdinal,
    required this.crossRowSelection,
    required List<FlarkSurfacePublicationRow> rows,
  }) : rows = List.unmodifiable(rows);

  final int sequence;
  final int interactionGeneration;
  final int revision;
  final int sourceGeneration;
  final bool semanticsCurrent;
  final int viewportPageIndex;
  final bool canPageForward;
  final bool canPageBackward;
  final bool pendingTableNavigationLocked;
  final int visibleUtf16Start;
  final String visibleSource;
  final int canonicalSelectionBaseUtf16;
  final int canonicalSelectionExtentUtf16;
  final int inputGlobalUtf16Start;
  final TextEditingValue inputValue;
  final int? activeOrdinal;
  final bool crossRowSelection;
  final List<FlarkSurfacePublicationRow> rows;

  int get canonicalCaretUtf16 => canonicalSelectionExtentUtf16;

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
    canonicalSelectionBaseUtf16: canonicalSelectionBaseUtf16,
    canonicalSelectionExtentUtf16: canonicalSelectionExtentUtf16,
    crossRowSelection: crossRowSelection,
    globalUtf16Start: globalUtf16Start,
    text: text,
    ordinal: ordinal,
    includeEditingState: includeEditingState,
  );
}

/// Pure source/display projection used by both controller publication and
/// immutable render snapshots. It owns no editor state and performs no I/O.
final class FlarkSurfaceProjection {
  const FlarkSurfaceProjection._();

  static bool selectionIntersects(
    FlarkSourceRange range,
    int base,
    int extent,
  ) {
    final start = math.min(base, extent);
    final end = math.max(base, extent);
    if (start == end) return range.start <= start && start <= range.end;
    return start < range.end && range.start < end;
  }

  static TextSelection projectedSelection({
    required List<FlarkSurfaceTextRun> runs,
    required int textLength,
    required int base,
    required int extent,
    required TextSelection inputSelection,
  }) {
    int project(int sourceOffset, TextAffinity affinity) {
      if (runs.isEmpty) return 0;
      var consumed = 0;
      for (final run in runs) {
        if (sourceOffset < run.sourceUtf16Start) return consumed;
        if (sourceOffset <= run.sourceUtf16End) {
          return (consumed +
                  run.textOffsetForSourceOffset(
                    sourceOffset,
                    affinity: affinity,
                  ))
              .clamp(0, textLength);
        }
        consumed += run.text.length;
      }
      return textLength;
    }

    final affinity = inputSelection.affinity;
    return TextSelection(
      baseOffset: project(base, affinity),
      extentOffset: project(extent, affinity),
      affinity: affinity,
      isDirectional: inputSelection.isDirectional,
    );
  }

  static ({String text, int globalStart, TextSelection selection})
  paintInputWindow({
    required TextEditingValue value,
    required int inputGlobalUtf16Start,
    int? sourceStart,
    int? sourceEnd,
  }) {
    final allowedStart = sourceStart == null
        ? 0
        : (sourceStart - inputGlobalUtf16Start).clamp(0, value.text.length);
    final allowedEnd = sourceEnd == null
        ? value.text.length
        : (sourceEnd - inputGlobalUtf16Start).clamp(
            allowedStart,
            value.text.length,
          );
    final allowedLength = allowedEnd - allowedStart;
    if (allowedLength <= _maximumPaintCodeUnits) {
      final text = value.text.substring(allowedStart, allowedEnd);
      return (
        text: text,
        globalStart: inputGlobalUtf16Start + allowedStart,
        selection: TextSelection(
          baseOffset: (value.selection.baseOffset - allowedStart).clamp(
            0,
            text.length,
          ),
          extentOffset: (value.selection.extentOffset - allowedStart).clamp(
            0,
            text.length,
          ),
          affinity: value.selection.affinity,
          isDirectional: value.selection.isDirectional,
        ),
      );
    }

    final selectionStart = math.min(
      value.selection.baseOffset,
      value.selection.extentOffset,
    );
    final selectionEnd = math.max(
      value.selection.baseOffset,
      value.selection.extentOffset,
    );
    final focus = value.selection.extentOffset.clamp(allowedStart, allowedEnd);
    var start = (focus - _maximumPaintCodeUnits ~/ 2).clamp(
      allowedStart,
      allowedEnd - _maximumPaintCodeUnits,
    );
    if (selectionStart >= allowedStart &&
        selectionEnd <= allowedEnd &&
        selectionEnd - selectionStart <= _maximumPaintCodeUnits) {
      start = math.min(start, selectionStart);
      start = math.max(
        allowedStart,
        math.max(start, selectionEnd - _maximumPaintCodeUnits),
      );
    }
    var end = start + _maximumPaintCodeUnits;
    if (start < value.text.length &&
        _isLowSurrogate(value.text.codeUnitAt(start))) {
      start += 1;
    }
    if (end < value.text.length &&
        _isLowSurrogate(value.text.codeUnitAt(end))) {
      end -= 1;
    }
    final text = value.text.substring(start, end);
    return (
      text: text,
      globalStart: inputGlobalUtf16Start + start,
      selection: TextSelection(
        baseOffset: (value.selection.baseOffset - start).clamp(0, text.length),
        extentOffset: (value.selection.extentOffset - start).clamp(
          0,
          text.length,
        ),
        affinity: value.selection.affinity,
        isDirectional: value.selection.isDirectional,
      ),
    );
  }

  static FlarkSurfaceRow neutralLineRow({
    String leadingText = '',
    required int globalUtf16Start,
    required String text,
    required int ordinal,
    required bool active,
    required bool selected,
    required int canonicalSelectionBaseUtf16,
    required int canonicalSelectionExtentUtf16,
    required TextSelection inputSelection,
  }) {
    final endingLength = text.endsWith('\r\n')
        ? 2
        : text.endsWith('\n') || text.endsWith('\r')
        ? 1
        : 0;
    final visibleText = endingLength == 0
        ? text
        : text.substring(0, text.length - endingLength);
    final whitespaceOnly = visibleText.isNotEmpty && visibleText.trim().isEmpty;
    final renderedText = whitespaceOnly ? '' : visibleText;
    final visibleEnd = globalUtf16Start + visibleText.length;
    final runs = <FlarkSurfaceTextRun>[
      FlarkSurfaceTextRun(
        text: renderedText,
        sourceUtf16Start: globalUtf16Start,
        sourceUtf16End: visibleEnd,
        sourceExact: !whitespaceOnly,
        styles: const {},
      ),
      if (endingLength > 0)
        FlarkSurfaceTextRun(
          text: '',
          sourceUtf16Start: visibleEnd,
          sourceUtf16End: visibleEnd + endingLength,
          sourceExact: false,
          styles: const {},
        ),
    ];
    return FlarkSurfaceRow(
      leadingText: leadingText,
      text: renderedText,
      globalUtf16Start: globalUtf16Start,
      kind: 0,
      headingLevel: null,
      blockQuoteDepth: null,
      codeBlock: null,
      thematicBreak: false,
      ordinal: ordinal,
      active: active,
      selection: selected
          ? projectedSelection(
              runs: runs,
              textLength: renderedText.length,
              base: canonicalSelectionBaseUtf16,
              extent: canonicalSelectionExtentUtf16,
              inputSelection: inputSelection,
            )
          : null,
      runs: runs,
    );
  }

  static FlarkSurfaceRow neutralRow({
    required int visibleUtf16Start,
    required String visibleSource,
    required int inputGlobalUtf16Start,
    required TextEditingValue inputValue,
    required int? activeOrdinal,
    required int canonicalSelectionBaseUtf16,
    required int canonicalSelectionExtentUtf16,
    required bool crossRowSelection,
    required int globalUtf16Start,
    required String text,
    required int ordinal,
    required bool includeEditingState,
  }) {
    final surfaceOrdinal = -ordinal - 1;
    final range = FlarkSourceRange(
      globalUtf16Start,
      globalUtf16Start + text.length,
    );
    String sliceVisible(int start, int end) {
      final localStart = (start - visibleUtf16Start).clamp(
        0,
        visibleSource.length,
      );
      final localEnd = (end - visibleUtf16Start).clamp(
        localStart,
        visibleSource.length,
      );
      return visibleSource.substring(localStart, localEnd);
    }

    if (includeEditingState &&
        crossRowSelection &&
        (selectionIntersects(
              range,
              canonicalSelectionBaseUtf16,
              canonicalSelectionExtentUtf16,
            ) ||
            activeOrdinal == surfaceOrdinal)) {
      return neutralLineRow(
        globalUtf16Start: range.start,
        text: sliceVisible(range.start, range.end),
        ordinal: surfaceOrdinal,
        active: activeOrdinal == surfaceOrdinal,
        selected: true,
        canonicalSelectionBaseUtf16: canonicalSelectionBaseUtf16,
        canonicalSelectionExtentUtf16: canonicalSelectionExtentUtf16,
        inputSelection: inputValue.selection,
      );
    }
    if (includeEditingState &&
        activeOrdinal == surfaceOrdinal &&
        canonicalSelectionExtentUtf16 >= globalUtf16Start &&
        canonicalSelectionExtentUtf16 <= globalUtf16Start + text.length) {
      final paintInput = paintInputWindow(
        value: inputValue,
        inputGlobalUtf16Start: inputGlobalUtf16Start,
        sourceStart: globalUtf16Start,
        sourceEnd: globalUtf16Start + text.length,
      );
      return neutralLineRow(
        globalUtf16Start: paintInput.globalStart,
        text: paintInput.text,
        ordinal: surfaceOrdinal,
        active: true,
        selected: true,
        canonicalSelectionBaseUtf16: canonicalSelectionBaseUtf16,
        canonicalSelectionExtentUtf16: canonicalSelectionExtentUtf16,
        inputSelection: inputValue.selection,
      );
    }
    return neutralLineRow(
      globalUtf16Start: globalUtf16Start,
      text: text,
      ordinal: surfaceOrdinal,
      active: false,
      selected: false,
      canonicalSelectionBaseUtf16: canonicalSelectionBaseUtf16,
      canonicalSelectionExtentUtf16: canonicalSelectionExtentUtf16,
      inputSelection: inputValue.selection,
    );
  }

  static bool _isLowSurrogate(int codeUnit) =>
      codeUnit >= 0xdc00 && codeUnit <= 0xdfff;
}
