import 'dart:convert';
import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flark/src/render_surface.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

final class MarkedSource {
  const MarkedSource(this.source, this.caret);

  factory MarkedSource.parse(String marked) {
    const marker = '¦';
    final caret = marked.indexOf(marker);
    if (caret < 0 || marked.indexOf(marker, caret + 1) >= 0) {
      throw ArgumentError.value(marked, 'marked', 'expected exactly one ¦');
    }
    return MarkedSource(marked.replaceFirst(marker, ''), caret);
  }

  final String source;
  final int caret;
}

typedef SurfaceRunSample = ({
  String text,
  int sourceStart,
  int sourceEnd,
  bool sourceExact,
  Set<String> styles,
});

SurfaceRunSample _captureRun(FlarkSurfaceTextRun run) => (
  text: run.text,
  sourceStart: run.sourceUtf16Start,
  sourceEnd: run.sourceUtf16End,
  sourceExact: run.sourceExact,
  styles: run.styles.map((style) => style.name).toSet(),
);

typedef SurfaceRowSample = ({
  int ordinal,
  int kind,
  int? headingLevel,
  int? quoteDepth,
  bool codeBlock,
  bool thematicBreak,
  bool listItem,
  bool neutral,
  String leadingText,
  String text,
  List<SurfaceRunSample> runs,
});

SurfaceRowSample _captureRow(FlarkSurfaceRow row, {bool neutral = false}) => (
  ordinal: row.ordinal,
  kind: row.kind,
  headingLevel: row.headingLevel,
  quoteDepth: row.blockQuoteDepth,
  codeBlock: row.codeBlock != null,
  thematicBreak: row.thematicBreak,
  listItem: row.listItem,
  neutral: neutral,
  leadingText: row.leadingText,
  text: row.text,
  runs: row.runs.map(_captureRun).toList(growable: false),
);

/// Captures the complete framework-neutral row plan visited by the production
/// render surface, including editor-owned neutral rows between parser rows.
/// A pending structural receipt and its settled parse can encode the same
/// visible blank line differently; comparing this paint plan keeps temporal
/// tests focused on visible equivalence instead of parser representation.
List<SurfaceRowSample> captureControllerSurfaceRows(
  FlarkEditorController controller, {
  bool includeEditingState = true,
}) {
  final captured = <SurfaceRowSample>[];
  final rows = controller.rows;
  if (rows.isEmpty) {
    final source = controller.visibleSource;
    var cursor = 0;
    var ordinal = 0;
    while (cursor <= source.length) {
      final newline = source.indexOf('\n', cursor);
      final end = newline == -1 ? source.length : newline + 1;
      captured.add(
        _captureRow(
          controller.neutralSurfaceRow(
            globalUtf16Start: controller.visibleUtf16Start + cursor,
            text: source.substring(cursor, end),
            ordinal: ordinal,
            includeEditingState: includeEditingState,
          ),
          neutral: true,
        ),
      );
      if (newline == -1) break;
      cursor = end;
      ordinal += 1;
    }
    return List.unmodifiable(captured);
  }

  var sourceCursor = controller.visibleUtf16Start;
  var precedingOwnsEditorBlockBoundary = false;
  for (final row in rows) {
    final range = controller.surfaceSourceRange(row);
    final presentations = controller.surfaceRowsFor(
      row,
      includeEditingState: includeEditingState,
    );
    if (presentations.isEmpty) continue;
    if (range.start > sourceCursor) {
      captured.addAll(
        _captureNeutralGapRows(
          controller,
          sourceCursor,
          range.start,
          hasPrecedingRow: sourceCursor > controller.visibleUtf16Start,
          precedingOwnsEditorBlockBoundary: precedingOwnsEditorBlockBoundary,
          hasFollowingRow: true,
          includeEditingState: includeEditingState,
        ),
      );
    }
    captured.addAll(presentations.map(_captureRow));
    precedingOwnsEditorBlockBoundary =
        presentations.length > 1 ||
        (presentations.isNotEmpty &&
            presentations.last.text.isEmpty &&
            presentations.last.leadingText.isEmpty &&
            presentations.last.kind == 5);
    if (range.end > sourceCursor) sourceCursor = range.end;
  }
  final visibleEnd =
      controller.visibleUtf16Start + controller.visibleSource.length;
  if (sourceCursor < visibleEnd) {
    captured.addAll(
      _captureNeutralGapRows(
        controller,
        sourceCursor,
        visibleEnd,
        hasPrecedingRow: true,
        precedingOwnsEditorBlockBoundary: precedingOwnsEditorBlockBoundary,
        hasFollowingRow: false,
        includeEditingState: includeEditingState,
      ),
    );
  }
  return List.unmodifiable(captured);
}

Iterable<SurfaceRowSample> _captureNeutralGapRows(
  FlarkEditorController controller,
  int globalStart,
  int globalEnd, {
  required bool hasPrecedingRow,
  required bool precedingOwnsEditorBlockBoundary,
  required bool hasFollowingRow,
  required bool includeEditingState,
}) sync* {
  final source = controller.visibleSource;
  final visibleStart = controller.visibleUtf16Start;
  final localStart = (globalStart - visibleStart).clamp(0, source.length);
  final localEnd = (globalEnd - visibleStart).clamp(localStart, source.length);
  final lines = <({int start, int end})>[];
  var cursor = localStart;
  while (cursor < localEnd) {
    final newline = source.indexOf('\n', cursor);
    final end = newline == -1 || newline >= localEnd ? localEnd : newline + 1;
    lines.add((start: cursor, end: end));
    cursor = end;
  }
  assert(!hasPrecedingRow || globalStart >= visibleStart);
  bool isWhitespaceLine(int index) {
    final line = lines[index];
    return source.substring(line.start, line.end).trim().isEmpty;
  }

  final emitted = List<bool>.filled(lines.length, true);
  if (hasPrecedingRow &&
      precedingOwnsEditorBlockBoundary &&
      lines.isNotEmpty &&
      isWhitespaceLine(0)) {
    emitted[0] = false;
  }
  if (hasFollowingRow &&
      lines.isNotEmpty &&
      isWhitespaceLine(lines.length - 1)) {
    emitted[lines.length - 1] = false;
  }
  for (var index = 1; index < lines.length; index += 1) {
    if (!isWhitespaceLine(index) && isWhitespaceLine(index - 1)) {
      emitted[index - 1] = false;
    }
  }
  for (var index = 0; index < lines.length; index += 1) {
    if (!emitted[index]) continue;
    final line = lines[index];
    var ordinal = 0;
    for (var offset = 0; offset < line.start; offset += 1) {
      if (source.codeUnitAt(offset) == 0x0a) ordinal += 1;
    }
    yield _captureRow(
      controller.neutralSurfaceRow(
        globalUtf16Start: visibleStart + line.start,
        text: source.substring(line.start, line.end),
        ordinal: ordinal,
        includeEditingState: includeEditingState,
      ),
      neutral: true,
    );
  }
}

String _withoutTerminalLineEnding(String text) {
  if (text.endsWith('\r\n')) {
    return text.substring(0, text.length - 2);
  }
  if (text.endsWith('\n') || text.endsWith('\r')) {
    return text.substring(0, text.length - 1);
  }
  return text;
}

String captureSurfaceRowsPresentation(Iterable<SurfaceRowSample> rows) => rows
    .map((row) => '${row.leadingText}${_withoutTerminalLineEnding(row.text)}')
    .join('\n');

String captureSurfaceRowsVisualManifest(Iterable<SurfaceRowSample> rows) =>
    jsonEncode(
      rows
          .map((row) {
            final visibleText = _withoutTerminalLineEnding(row.text);
            final runs = <Map<String, Object?>>[];
            String? precedingStyleKey;
            for (var index = 0; index < row.runs.length; index += 1) {
              final run = row.runs[index];
              final text = index == row.runs.length - 1
                  ? _withoutTerminalLineEnding(run.text)
                  : run.text;
              if (text.isEmpty && run.styles.isEmpty) continue;
              final styles = run.styles.toList()..sort();
              final styleKey = styles.join('\u0000');
              if (runs.isNotEmpty && styleKey == precedingStyleKey) {
                runs.last['text'] = '${runs.last['text']}$text';
              } else {
                runs.add({'text': text, 'styles': styles});
                precedingStyleKey = styleKey;
              }
            }
            if ((row.listItem || row.kind == 14) &&
                row.headingLevel == null &&
                !row.codeBlock &&
                !row.thematicBreak) {
              // The mounted list Return lane proves parser Item (kind 14)
              // and its receipt-backed content surface (kind 5) resolve to
              // identical text style and geometry. The mounted empty-list
              // Backspace lane additionally proves a top-level unstyled list
              // and its exact fallback paint the same combined marker text,
              // rectangles, and block style.
              if (row.quoteDepth == null &&
                  runs.every(
                    (run) => (run['styles']! as List<Object?>).isEmpty,
                  )) {
                return <String, Object?>{
                  'plainBlock': '${row.leadingText}$visibleText',
                };
              }
              // Nested/quoted or styled list rows retain their semantic
              // identity because that can materially change paint.
              return <String, Object?>{
                'listBlock': true,
                'quoteDepth': row.quoteDepth,
                'leadingText': row.leadingText,
                'text': visibleText,
                'runs': runs,
              };
            }
            final plainUnstyledBlock =
                (row.kind == 0 || row.kind == 5) &&
                row.headingLevel == null &&
                row.quoteDepth == null &&
                !row.codeBlock &&
                !row.thematicBreak &&
                !row.listItem &&
                row.runs.every((run) => run.styles.isEmpty);
            if (plainUnstyledBlock) {
              // The mounted geometry lane proves exact-source fallback and a
              // plain Paragraph resolve to the same block style and geometry.
              // Mapping-run segmentation and the parser's neutral flag are
              // not visible; authored marker text remains visible and still
              // differs from a projected styled result.
              return <String, Object?>{
                'plainBlock': '${row.leadingText}$visibleText',
              };
            }
            return <String, Object?>{
              'kind': row.kind,
              'headingLevel': row.headingLevel,
              'quoteDepth': row.quoteDepth,
              'codeBlock': row.codeBlock,
              'thematicBreak': row.thematicBreak,
              'listItem': row.listItem,
              'neutral': row.neutral,
              'leadingText': row.leadingText,
              'text': visibleText,
              'runs': runs,
            };
          })
          .toList(growable: false),
    );

String captureControllerPresentation(
  FlarkEditorController controller, {
  bool includeEditingState = true,
}) => captureSurfaceRowsPresentation(
  captureControllerSurfaceRows(
    controller,
    includeEditingState: includeEditingState,
  ),
);

String captureControllerVisualManifest(
  FlarkEditorController controller, {
  bool includeEditingState = true,
}) => captureSurfaceRowsVisualManifest(
  captureControllerSurfaceRows(
    controller,
    includeEditingState: includeEditingState,
  ),
);

String _rowManifest(SurfaceRowSample row) => <Object?>[
  row.ordinal,
  row.kind,
  row.headingLevel,
  row.quoteDepth,
  row.codeBlock,
  row.thematicBreak,
  row.listItem,
  row.neutral,
  row.leadingText,
  row.text,
  row.runs
      .map(
        (run) =>
            '${run.sourceStart}:${run.sourceEnd}:'
            '${run.sourceExact ? 'exact' : 'mapped'}:'
            '${run.styles.toList()..sort()}:${run.text}',
      )
      .join('|'),
].join('~');

/// One synchronous state made observable by the controller.
/// This deliberately contains no asynchronous source read: such a read waits
/// for the mutation tail and would associate a later source with this sample.
final class PublicationSample {
  const PublicationSample({
    required this.sequence,
    required this.revision,
    required this.status,
    required this.pendingEdits,
    required this.semanticsCurrent,
    required this.projectionContinuityActive,
    required this.structuralSurfaceCount,
    required this.structuralSurfaceContinuityActive,
    required this.publicationCertificationBarrierActive,
    required this.visibleSource,
    required this.visibleStart,
    required this.sourceUtf16Length,
    required this.inputGlobalStart,
    required this.inputValue,
    required this.inputWindowTextSha256,
    required this.globalSelectionBase,
    required this.globalSelectionExtent,
    required this.hasOversizedSelection,
    required this.resyncCount,
    required this.lastError,
    required this.rows,
  });

  factory PublicationSample.capture(
    FlarkEditorController controller,
    int sequence,
  ) => PublicationSample(
    sequence: sequence,
    revision: controller.revision,
    status: controller.status,
    pendingEdits: controller.pendingEdits,
    semanticsCurrent: controller.semanticsCurrent,
    projectionContinuityActive: controller.debugProjectionContinuityActive,
    structuralSurfaceCount: controller.debugStructuralSurfaceCount,
    structuralSurfaceContinuityActive:
        controller.debugStructuralSurfaceContinuityActive,
    publicationCertificationBarrierActive:
        controller.debugPublicationCertificationBarrierActive,
    visibleSource: controller.visibleSource,
    visibleStart: controller.visibleUtf16Start,
    sourceUtf16Length: controller.sourceUtf16Length,
    inputGlobalStart: controller.inputWindowShadow.globalUtf16Start,
    inputValue: controller.inputValue,
    inputWindowTextSha256: controller.inputWindowShadow.windowTextSha256,
    globalSelectionBase: controller.globalSelectionBase,
    globalSelectionExtent: controller.globalSelectionExtent,
    hasOversizedSelection: controller.hasOversizedSelection,
    resyncCount: controller.resyncCount,
    lastError: controller.lastError,
    rows: captureControllerSurfaceRows(controller),
  );

  final int sequence;
  final int revision;
  final FlarkEditorStatus status;
  final int pendingEdits;
  final bool semanticsCurrent;
  final bool projectionContinuityActive;
  final int structuralSurfaceCount;
  final bool structuralSurfaceContinuityActive;
  final bool publicationCertificationBarrierActive;
  final String visibleSource;
  final int visibleStart;
  final int sourceUtf16Length;
  final int inputGlobalStart;
  final TextEditingValue inputValue;
  final String inputWindowTextSha256;
  final int globalSelectionBase;
  final int globalSelectionExtent;
  final bool hasOversizedSelection;
  final int resyncCount;
  final Object? lastError;
  final List<SurfaceRowSample> rows;

  String get presentation => captureSurfaceRowsPresentation(rows);

  String get visualManifest => captureSurfaceRowsVisualManifest(rows);

  void expectMechanicallyValid({bool requirePublishedShadowMatch = true}) {
    // This validator also runs synchronously from a ChangeNotifier listener.
    // `expectSync` preserves immediate failure without entering Flutter's
    // guarded async API while a pump is delivering a native completion.
    expectSync(pendingEdits, greaterThanOrEqualTo(0));
    expectSync(status, isNot(FlarkEditorStatus.faulted));
    expectSync(lastError, isNull);
    expectSync(resyncCount, 0);
    if (requirePublishedShadowMatch) {
      expectSync(
        inputWindowTextSha256,
        flarkWindowTextSha256(inputValue.text),
        reason:
            'an observable publication must reconcile the platform shadow '
            'with the exposed input text',
      );
    }
    final projectedUtf16Length = <int>[
      sourceUtf16Length,
      visibleStart + visibleSource.length,
      inputGlobalStart + inputValue.text.length,
    ].reduce((left, right) => left > right ? left : right);
    expectSync(visibleStart, inInclusiveRange(0, projectedUtf16Length));
    expectSync(
      visibleStart + visibleSource.length,
      lessThanOrEqualTo(projectedUtf16Length),
    );
    expectSync(globalSelectionBase, inInclusiveRange(0, projectedUtf16Length));
    expectSync(
      globalSelectionExtent,
      inInclusiveRange(0, projectedUtf16Length),
    );
    final selection = inputValue.selection;
    expectSync(selection.isValid, isTrue);
    expectSync(selection.start, inInclusiveRange(0, inputValue.text.length));
    expectSync(selection.end, inInclusiveRange(0, inputValue.text.length));
    if (!hasOversizedSelection) {
      expectSync(
        inputGlobalStart + selection.baseOffset,
        globalSelectionBase,
        reason: 'the platform input base must represent the canonical base',
      );
      expectSync(
        inputGlobalStart + selection.extentOffset,
        globalSelectionExtent,
        reason: 'the platform input extent must represent the canonical extent',
      );
    }
    final composing = inputValue.composing;
    if (composing != TextRange.empty) {
      expectSync(composing.isValid, isTrue);
      expectSync(composing.start, inInclusiveRange(0, inputValue.text.length));
      expectSync(composing.end, inInclusiveRange(0, inputValue.text.length));
    }
    for (final run in rows.expand((row) => row.runs)) {
      expectSync(run.sourceStart, lessThanOrEqualTo(run.sourceEnd));
      expectSync(run.sourceStart, greaterThanOrEqualTo(visibleStart));
      expectSync(
        run.sourceEnd,
        lessThanOrEqualTo(visibleStart + visibleSource.length),
      );
      if (!run.sourceExact) continue;
      final localStart = run.sourceStart - visibleStart;
      final localEnd = run.sourceEnd - visibleStart;
      expectSync(
        visibleSource.substring(localStart, localEnd),
        run.text,
        reason: 'exact run disagrees with its represented source',
      );
    }
  }
}

final class ActionTrace {
  const ActionTrace({
    required this.before,
    required this.publications,
    required this.callbackReturn,
  });

  final PublicationSample before;
  final List<PublicationSample> publications;
  final PublicationSample callbackReturn;

  Iterable<PublicationSample> get observableStates sync* {
    yield before;
    yield* publications;
    yield callbackReturn;
  }
}

final class LiveEditorTransitionProbe {
  LiveEditorTransitionProbe._(
    this.controller,
    this.libraryPath,
    this._platformValue,
  ) {
    controller.addListener(_recordPublication);
    _recordPublication();
  }

  static Future<LiveEditorTransitionProbe> open(
    String marked, {
    String? libraryPath,
  }) async {
    final parsed = MarkedSource.parse(marked);
    final resolvedLibrary =
        libraryPath ?? Platform.environment['FLARK_V4_LIBRARY_PATH']!;
    final controller = await FlarkEditorController.open(
      parsed.source,
      libraryPath: resolvedLibrary,
    );
    await controller.continueParsing();
    final row = controller.rows.firstWhere(
      (candidate) =>
          candidate.editableUtf16 != null &&
          candidate.editableUtf16!.start <= parsed.caret &&
          parsed.caret <= candidate.editableUtf16!.end,
      orElse: () => throw StateError(
        'caret ${parsed.caret} is not in an editable certified row',
      ),
    );
    controller.activateRow(row, parsed.caret);
    return LiveEditorTransitionProbe._(
      controller,
      resolvedLibrary,
      controller.inputValue,
    );
  }

  final FlarkEditorController controller;
  final String libraryPath;
  TextEditingValue _platformValue;
  final List<PublicationSample> publications = [];
  int _sampleSequence = 0;

  String get semanticManifest => _semanticManifest(controller);

  void _recordPublication() {
    final sample = PublicationSample.capture(controller, _sampleSequence++);
    sample.expectMechanicallyValid();
    publications.add(sample);
  }

  ActionTrace observe(void Function() action) {
    final before = PublicationSample.capture(controller, _sampleSequence++);
    // This is a synchronous sampling point, not a controller publication. A
    // same-burst callback can begin while the platform shadow is already one
    // provisional value ahead of the last exposed controller window.
    before.expectMechanicallyValid(requirePublishedShadowMatch: false);
    final publicationStart = publications.length;
    action();
    final callbackReturn = PublicationSample.capture(
      controller,
      _sampleSequence++,
    );
    // Callback return is intentionally not an observable publication. While
    // parser certification is in flight, the text service may own a newer
    // provisional value than the controller is allowed to expose or paint.
    callbackReturn.expectMechanicallyValid(requirePublishedShadowMatch: false);
    return ActionTrace(
      before: before,
      publications: List.unmodifiable(publications.skip(publicationStart)),
      callbackReturn: callbackReturn,
    );
  }

  Future<ActionTrace> observeAsync(Future<void> Function() action) async {
    final before = PublicationSample.capture(controller, _sampleSequence++);
    before.expectMechanicallyValid(requirePublishedShadowMatch: false);
    final publicationStart = publications.length;
    await action();
    final callbackReturn = PublicationSample.capture(
      controller,
      _sampleSequence++,
    );
    callbackReturn.expectMechanicallyValid(requirePublishedShadowMatch: false);
    return ActionTrace(
      before: before,
      publications: List.unmodifiable(publications.skip(publicationStart)),
      callbackReturn: callbackReturn,
    );
  }

  List<ActionTrace> typeText(String text) {
    final traces = <ActionTrace>[];
    for (final rune in text.runes) {
      final character = String.fromCharCode(rune);
      final before = _platformValue;
      final selection = before.selection;
      final delta = selection.isCollapsed
          ? TextEditingDeltaInsertion(
              oldText: before.text,
              textInserted: character,
              insertionOffset: selection.start,
              selection: TextSelection.collapsed(
                offset: selection.start + character.length,
              ),
              composing: TextRange.empty,
            )
          : TextEditingDeltaReplacement(
              oldText: before.text,
              replacementText: character,
              replacedRange: TextRange(
                start: selection.start,
                end: selection.end,
              ),
              selection: TextSelection.collapsed(
                offset: selection.start + character.length,
              ),
              composing: TextRange.empty,
            );
      traces.add(observe(() => controller.applyDeltas([delta])));
      _platformValue = delta.apply(before);
    }
    return traces;
  }

  ActionTrace pressReturn() {
    final before = _platformValue;
    final selection = before.selection;
    final delta = selection.isCollapsed
        ? TextEditingDeltaInsertion(
            oldText: before.text,
            textInserted: '\n',
            insertionOffset: selection.start,
            selection: TextSelection.collapsed(offset: selection.start + 1),
            composing: TextRange.empty,
          )
        : TextEditingDeltaReplacement(
            oldText: before.text,
            replacementText: '\n',
            replacedRange: TextRange(
              start: selection.start,
              end: selection.end,
            ),
            selection: TextSelection.collapsed(offset: selection.start + 1),
            composing: TextRange.empty,
          );
    final trace = observe(() {
      controller.applyDeltas([delta]);
      controller.observePlatformNewlineAction();
    });
    _platformValue = delta.apply(before);
    return trace;
  }

  ActionTrace pressBackspace() {
    final trace = observe(controller.deleteBackward);
    _platformValue = controller.inputValue;
    return trace;
  }

  ActionTrace pressDelete() {
    final trace = observe(controller.deleteForward);
    _platformValue = controller.inputValue;
    return trace;
  }

  ActionTrace replaceSelection(String replacement) {
    final before = _platformValue;
    final selection = before.selection;
    final delta = selection.isCollapsed
        ? TextEditingDeltaInsertion(
            oldText: before.text,
            textInserted: replacement,
            insertionOffset: selection.start,
            selection: TextSelection.collapsed(
              offset: selection.start + replacement.length,
            ),
            composing: TextRange.empty,
          )
        : TextEditingDeltaReplacement(
            oldText: before.text,
            replacementText: replacement,
            replacedRange: TextRange(
              start: selection.start,
              end: selection.end,
            ),
            selection: TextSelection.collapsed(
              offset: selection.start + replacement.length,
            ),
            composing: TextRange.empty,
          );
    final trace = observe(() => controller.applyDeltas([delta]));
    _platformValue = delta.apply(before);
    return trace;
  }

  Future<ActionTrace> undo() async {
    final trace = await observeAsync(() async {
      if (!await controller.undo()) {
        throw StateError('undo was unavailable');
      }
    });
    _platformValue = controller.inputValue;
    return trace;
  }

  Future<ActionTrace> redo() async {
    final trace = await observeAsync(() async {
      if (!await controller.redo()) {
        throw StateError('redo was unavailable');
      }
    });
    _platformValue = controller.inputValue;
    return trace;
  }

  void moveCaret(int globalUtf16Offset) {
    final row = controller.rows.firstWhere(
      (candidate) {
        final editable = candidate.editableUtf16;
        return editable != null &&
            editable.start <= globalUtf16Offset &&
            globalUtf16Offset <= editable.end;
      },
      orElse: () => throw StateError(
        'caret $globalUtf16Offset is not in an editable certified row',
      ),
    );
    controller.activateRow(row, globalUtf16Offset);
    _platformValue = controller.inputValue;
  }

  Future<void> selectRange(int baseUtf16, int extentUtf16) async {
    final start = baseUtf16 < extentUtf16 ? baseUtf16 : extentUtf16;
    final end = baseUtf16 < extentUtf16 ? extentUtf16 : baseUtf16;
    final row = controller.rows.firstWhere(
      (candidate) {
        final editable = candidate.editableUtf16;
        return editable != null &&
            editable.start <= start &&
            end <= editable.end;
      },
      orElse: () => throw StateError(
        'selection $baseUtf16..$extentUtf16 is not in one editable row',
      ),
    );
    controller.activateRow(row, baseUtf16, selectionExtent: extentUtf16);
    _platformValue = controller.inputValue;
    await controller.debugWaitForMutationSettled();
  }

  Future<PublicationSample> presentationSettled() async {
    await controller.debugWaitForPresentationSettled();
    _platformValue = controller.inputValue;
    final sample = PublicationSample.capture(controller, _sampleSequence++);
    sample.expectMechanicallyValid();
    return sample;
  }

  Future<void> mutationSettled() async {
    await controller.debugWaitForMutationSettled();
    // A live TextInputConnection receives the controller's authoritative
    // post-command window before the user's next key. Keep the probe's
    // simulated platform shadow at that same boundary when a scenario
    // deliberately pauses between actions.
    _platformValue = controller.inputValue;
  }

  Future<void> expectSourceAndCaret(String marked) async {
    final expected = MarkedSource.parse(marked);
    await controller.debugWaitForMutationSettled();
    expect(await controller.readSource(), expected.source);
    final selection = await controller.resolveCanonicalSelection();
    expect(selection?.base, expected.caret);
    expect(selection?.extent, expected.caret);
  }

  Future<void> expectHealthy() async {
    await controller.debugWaitForMutationSettled();
    expect(controller.status, isNot(FlarkEditorStatus.faulted));
    expect(controller.lastError, isNull);
    expect(controller.resyncCount, 0);
  }

  Future<void> expectConvergesWithCleanRebuild() async {
    await controller.debugWaitForPresentationSettled();
    final source = await controller.readSource();
    final fresh = await FlarkEditorController.open(
      source,
      libraryPath: libraryPath,
    );
    try {
      await fresh.continueParsing();
      expect(_semanticManifest(controller), _semanticManifest(fresh));
    } finally {
      await fresh.close();
    }
  }

  Future<void> close() async {
    controller.removeListener(_recordPublication);
    await controller.close();
  }
}

String _semanticManifest(FlarkEditorController controller) => controller.rows
    .map((row) => controller.surfaceRow(row, includeEditingState: false))
    .map(_captureRow)
    .map(_rowManifest)
    .join('\n');

final class MountedTransitionRecorder {
  MountedTransitionRecorder._(this.tester, this.probe);

  static Future<MountedTransitionRecorder> mount(
    WidgetTester tester,
    LiveEditorTransitionProbe probe, {
    Size size = const Size(420, 600),
    TextDirection textDirection = TextDirection.ltr,
  }) async {
    await tester.binding.setSurfaceSize(size);
    final recorder = MountedTransitionRecorder._(tester, probe);
    await tester.pumpWidget(
      Directionality(
        textDirection: textDirection,
        child: SizedBox.expand(
          child: FlarkRenderSurfaceWidget(
            controller: probe.controller,
            textStyle: const TextStyle(fontSize: 17, height: 1.45),
            padding: EdgeInsets.zero,
            caretColor: const Color(0xff246bfd),
            selectionColor: const Color(0x40246bfd),
            debugPaintObserver: recorder._recordPaint,
          ),
        ),
      ),
    );
    await tester.pump();
    recorder.paints.clear();
    return recorder;
  }

  final WidgetTester tester;
  final LiveEditorTransitionProbe probe;
  final List<FlarkSurfacePaintObservation> paints = [];

  void _recordPaint(FlarkSurfacePaintObservation paint) {
    if (paint.caretRect != null) {
      expect(
        paint.caretSourceUtf16,
        paint.canonicalSelectionExtentUtf16,
        reason:
            'every painted caret must represent the controller selection in '
            'the same frame',
      );
    }
    paints.add(paint);
  }

  Future<List<ActionTrace>> typeText(String text) async =>
      (await tester.runAsync(() async => probe.typeText(text)))!;

  Future<List<ActionTrace>> typeTextAndPumpEachCharacter(String text) async {
    final traces = <ActionTrace>[];
    for (final rune in text.runes) {
      traces.addAll(await typeText(String.fromCharCode(rune)));
      await pumpImmediate();
    }
    return traces;
  }

  Future<ActionTrace> pressReturn() async =>
      (await tester.runAsync(() async => probe.pressReturn()))!;

  Future<ActionTrace> pressBackspace() async =>
      (await tester.runAsync(() async => probe.pressBackspace()))!;

  Future<ActionTrace> pressDelete() async =>
      (await tester.runAsync(() async => probe.pressDelete()))!;

  Future<ActionTrace> replaceSelection(String replacement) async =>
      (await tester.runAsync(() async => probe.replaceSelection(replacement)))!;

  Future<ActionTrace> undo() async => (await tester.runAsync(probe.undo))!;

  Future<ActionTrace> redo() async => (await tester.runAsync(probe.redo))!;

  Future<void> moveCaret(int globalUtf16Offset) async {
    await tester.runAsync(() async => probe.moveCaret(globalUtf16Offset));
    await tester.pump();
  }

  Future<void> selectRange(int baseUtf16, int extentUtf16) async {
    await tester.runAsync(() => probe.selectRange(baseUtf16, extentUtf16));
    await tester.pump();
  }

  Future<void> pumpImmediate() => tester.pump();

  Future<void> pumpMutationSettled() async {
    await tester.runAsync(probe.mutationSettled);
    await tester.pump();
  }

  Future<void> pumpPresentationSettled() async {
    await tester.runAsync(probe.presentationSettled);
    await tester.pump();
  }

  Future<void> close() async {
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.binding.setSurfaceSize(null);
  }
}
