@Tags(<String>['benchmark'])
library;

import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/markdown/markdown.dart';
import 'package:flark/src/v2/projection/projection.dart';

import '../../../../tool/parser_research/dart/persistent_document.dart';

void main() {
  for (final mode in _FocusedSyntaxMode.values) {
    testWidgets(
      'real Comrak reaches styled pixels before the next paint (${mode.name})',
      (tester) async {
        final backend = FlarkNativeComrakParseBackend.tryLoad();
        expect(
          backend,
          isNotNull,
          reason: 'the product-feel receipt requires the real native parser',
        );
        final builds = <int, int>{};
        final model = _ProductFeelModel(
          backend: backend!,
          blockCount: 50000,
          mode: mode,
          activeSource: '**bold*',
        );
        addTearDown(model.dispose);

        await tester.pumpWidget(
          _testSurface(_ProductFeelViewport(model: model, builds: builds)),
        );
        await tester.pump();

        expect(find.byType(EditableText), findsOneWidget);
        expect(
          find.byType(RichText).evaluate().length,
          lessThan(80),
          reason: 'the viewport mounted a document-sized render surface',
        );
        final adjacentBuilds = builds[1];
        final editableFinder = find.byKey(_activeEditableKey);
        final stateBefore = tester.state<EditableTextState>(editableFinder);
        stateBefore.widget.focusNode.requestFocus();
        await tester.pump();
        expect(tester.testTextInput.hasAnyClients, isTrue);
        expect(stateBefore.widget.controller.text, '**bold*');

        final urgent = Stopwatch()..start();
        stateBefore.updateEditingValue(
          const TextEditingValue(
            text: '**bold**',
            selection: TextSelection.collapsed(offset: 8),
          ),
        );
        urgent.stop();

        // The parser/projection transaction is already authoritative before
        // the first frame after the platform delta is allowed to paint.
        expect(model.authoritativeRevision, model.source.revision);
        expect(model.presentation.hasStrong, isTrue);
        expect(model.lastUrgentMicros, greaterThan(0));

        final firstPaint = Stopwatch()..start();
        await tester.pump();
        firstPaint.stop();

        final stateAfter = tester.state<EditableTextState>(editableFinder);
        expect(identical(stateBefore, stateAfter), isTrue);
        expect(tester.testTextInput.hasAnyClients, isTrue);
        expect(builds[1], adjacentBuilds);
        expect(
          stateAfter.widget.controller.text,
          mode == _FocusedSyntaxMode.hidden ? 'bold' : '**bold**',
        );
        expect(_containsBold(_textSpan(tester, stateAfter.widget)), isTrue);
        expect(
          model.source.substring(0, model.activeSource.length),
          '**bold**',
        );

        final boundary = tester.renderObject<RenderRepaintBoundary>(
          find.byKey(_activePaintBoundaryKey),
        );
        final image = await boundary.toImage(pixelRatio: 1);
        expect(image.width, greaterThan(0));
        expect(image.height, greaterThan(0));
        image.dispose();

        final urgentSamples = <int>[urgent.elapsedMicroseconds];
        final pumpSamples = <int>[firstPaint.elapsedMicroseconds];
        for (var iteration = 0; iteration < 40; iteration += 1) {
          final current = stateAfter.widget.controller.value;
          final offset = mode == _FocusedSyntaxMode.hidden
              ? current.text.length
              : model.activeSource.length - 2;
          final next = current.copyWith(
            text: current.text.replaceRange(offset, offset, 'x'),
            selection: TextSelection.collapsed(offset: offset + 1),
            composing: TextRange.empty,
          );
          stateAfter.updateEditingValue(next);
          urgentSamples.add(model.lastUrgentMicros);
          final pump = Stopwatch()..start();
          await tester.pump();
          pump.stop();
          pumpSamples.add(pump.elapsedMicroseconds);
        }

        urgentSamples.sort();
        pumpSamples.sort();
        expect(model.presentation.hasStrong, isTrue);
        expect(tester.testTextInput.hasAnyClients, isTrue);
        expect(builds[1], adjacentBuilds);
        debugPrint(
          'flark_product_feel mode=${mode.name} blocks=${model.blockCount} '
          'source_bytes=${model.source.utf8Length} '
          'urgent_p50_us=${_percentile(urgentSamples, 50)} '
          'urgent_p95_us=${_percentile(urgentSamples, 95)} '
          'urgent_max_us=${urgentSamples.last} '
          'parse_p95_us=${_percentile(model.parseMicros, 95)} '
          'pump_p50_us=${_percentile(pumpSamples, 50)} '
          'pump_p95_us=${_percentile(pumpSamples, 95)} '
          'pump_max_us=${pumpSamples.last} '
          'active_builds=${builds[0]} adjacent_builds=${builds[1]}',
        );
      },
    );

    testWidgets(
      'composition reveals source without replacing the input host (${mode.name})',
      (tester) async {
        final backend = FlarkNativeComrakParseBackend.tryLoad();
        expect(backend, isNotNull);
        final model = _ProductFeelModel(
          backend: backend!,
          blockCount: 50000,
          mode: mode,
          activeSource: '**bold**',
        );
        addTearDown(model.dispose);

        await tester.pumpWidget(
          _testSurface(_ProductFeelViewport(model: model, builds: {})),
        );
        await tester.pump();
        final finder = find.byKey(_activeEditableKey);
        final stateBefore = tester.state<EditableTextState>(finder);
        stateBefore.widget.focusNode.requestFocus();
        await tester.pump();

        final current = stateBefore.widget.controller.value;
        final insertionOffset = mode == _FocusedSyntaxMode.hidden ? 1 : 3;
        stateBefore.updateEditingValue(
          current.copyWith(
            text: current.text.replaceRange(
              insertionOffset,
              insertionOffset,
              'é',
            ),
            selection: TextSelection.collapsed(offset: insertionOffset + 1),
            composing: TextRange(
              start: insertionOffset,
              end: insertionOffset + 1,
            ),
          ),
        );
        expect(model.authoritativeRevision, model.source.revision);
        await tester.pump();

        final stateDuring = tester.state<EditableTextState>(finder);
        expect(identical(stateBefore, stateDuring), isTrue);
        expect(tester.testTextInput.hasAnyClients, isTrue);
        expect(stateDuring.widget.controller.text, '**béold**');
        expect(
          stateDuring.widget.controller.value.composing,
          isNot(TextRange.empty),
        );
        expect(_containsBold(_textSpan(tester, stateDuring.widget)), isTrue);

        stateDuring.updateEditingValue(
          stateDuring.widget.controller.value.copyWith(
            composing: TextRange.empty,
          ),
        );
        await tester.pump();

        final stateAfter = tester.state<EditableTextState>(finder);
        expect(identical(stateBefore, stateAfter), isTrue);
        expect(tester.testTextInput.hasAnyClients, isTrue);
        expect(
          stateAfter.widget.controller.text,
          mode == _FocusedSyntaxMode.hidden ? 'béold' : '**béold**',
        );
      },
    );

    testWidgets(
      'a real fence transition keeps the same input host (${mode.name})',
      (tester) async {
        final backend = FlarkNativeComrakParseBackend.tryLoad();
        expect(backend, isNotNull);
        final model = _ProductFeelModel(
          backend: backend!,
          blockCount: 50000,
          mode: mode,
          activeSource: '``',
        );
        addTearDown(model.dispose);

        await tester.pumpWidget(
          _testSurface(_ProductFeelViewport(model: model, builds: {})),
        );
        await tester.pump();
        final finder = find.byKey(_activeEditableKey);
        final before = tester.state<EditableTextState>(finder);
        before.widget.focusNode.requestFocus();
        await tester.pump();
        expect(model.presentation.isCodeBlock, isFalse);

        before.updateEditingValue(
          const TextEditingValue(
            text: '```',
            selection: TextSelection.collapsed(offset: 3),
          ),
        );
        expect(model.presentation.isCodeBlock, isTrue);
        await tester.pump();

        final after = tester.state<EditableTextState>(finder);
        expect(identical(before, after), isTrue);
        expect(tester.testTextInput.hasAnyClients, isTrue);
        expect(
          after.widget.controller.text,
          mode == _FocusedSyntaxMode.hidden ? '' : '```',
        );
        expect(
          tester
              .widget<DecoratedBox>(find.byKey(_activeDecorationKey))
              .decoration,
          isA<BoxDecoration>().having(
            (decoration) => decoration.color,
            'color',
            const Color(0xFFF3F4F6),
          ),
        );
      },
    );
  }
}

enum _FocusedSyntaxMode { hidden, reveal }

const _activeEditableKey = Key('product-feel-active-editable');
const _activePaintBoundaryKey = Key('product-feel-active-paint-boundary');
const _activeDecorationKey = Key('product-feel-active-decoration');

Widget _testSurface(Widget child) {
  return Directionality(
    textDirection: TextDirection.ltr,
    child: MediaQuery(
      data: const MediaQueryData(size: Size(600, 600)),
      child: Center(child: SizedBox(width: 600, height: 600, child: child)),
    ),
  );
}

final class _ProductFeelModel {
  _ProductFeelModel({
    required this.backend,
    required this.blockCount,
    required this.mode,
    required String activeSource,
  }) : _activeSource = activeSource,
       _source = PrototypePersistentDocument.fromString(
         _documentSource(blockCount, activeSource),
       ) {
    _sourceSelection = FlarkSelection.collapsed(activeSource.length);
    _parseAndPublish(composing: TextRange.empty, forceReveal: false);
  }

  final FlarkNativeComrakParseBackend backend;
  final int blockCount;
  final _FocusedSyntaxMode mode;
  final FlarkProjectedTextEditAdapter _adapter =
      const FlarkProjectedTextEditAdapter();
  final ValueNotifier<_ActivePayload?> active = ValueNotifier(null);
  final List<int> parseMicros = [];

  PrototypePersistentDocument _source;
  String _activeSource;
  late FlarkSelection _sourceSelection;
  late _Presentation _presentation;
  var _forceReveal = false;
  var lastUrgentMicros = 0;

  PrototypePersistentDocument get source => _source;
  String get activeSource => _activeSource;
  _Presentation get presentation => _presentation;
  int get authoritativeRevision => _presentation.result.revision;

  String blockAt(int index) {
    if (index == 0) return _activeSource;
    return _ordinaryBlock(index);
  }

  void applyPlatformValue(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) {
    final urgent = Stopwatch()..start();
    final wasRevealed = _presentation.revealed;
    final textChanged = oldValue.text != newValue.text;
    TextRange composingSource = TextRange.empty;

    if (textChanged) {
      final before = _activeSource;
      if (wasRevealed) {
        _activeSource = newValue.text;
        _sourceSelection = _flarkSelection(newValue.selection);
      } else {
        final resolution = _adapter.resolveDisplayEdit(
          currentMarkdown: before,
          projection: _presentation.projection,
          oldDisplayText: oldValue.text,
          newDisplayText: newValue.text,
          sourceSelectionBefore: _sourceSelection,
          newDisplayCaret: newValue.selection.isCollapsed
              ? newValue.selection.extentOffset
              : null,
        );
        if (resolution == null) {
          throw StateError('authoritative projection could not map input');
        }
        final transaction = resolution.transaction;
        _activeSource = transaction
            .applyToDocument(FlarkDocument.fromMarkdown(before))
            .markdown;
        _sourceSelection =
            transaction.selectionAfter ??
            transaction.mapSelection(_sourceSelection);
      }

      final diff = _TextDiff.between(before, _activeSource);
      _source = _source
          .apply(
            PrototypeDocumentEdit(
              baseRevision: _source.revision,
              startUtf16: diff.start,
              endUtf16: diff.start + diff.deletedLength,
              replacement: diff.replacement,
            ),
          )
          .document;

      final nextResult = _parse();
      final nextProjection = FlarkProjection.fromParseResult(nextResult);
      if (_hasComposition(newValue.composing)) {
        composingSource = wasRevealed
            ? newValue.composing
            : TextRange(
                start: nextProjection.displayToSourceOffset(
                  newValue.composing.start,
                  affinity: FlarkMapAffinity.upstream,
                ),
                end: nextProjection.displayToSourceOffset(
                  newValue.composing.end,
                  affinity: FlarkMapAffinity.downstream,
                ),
              );
      }
      _publish(
        result: nextResult,
        projection: nextProjection,
        composing: composingSource,
        forceReveal: _hasComposition(newValue.composing),
      );
    } else {
      if (wasRevealed) {
        _sourceSelection = _flarkSelection(newValue.selection);
        composingSource = newValue.composing;
      } else {
        _sourceSelection = _presentation.projection.displaySelectionToSource(
          _flarkSelection(newValue.selection),
        );
        if (_hasComposition(newValue.composing)) {
          composingSource = TextRange(
            start: _presentation.projection.displayToSourceOffset(
              newValue.composing.start,
              affinity: FlarkMapAffinity.upstream,
            ),
            end: _presentation.projection.displayToSourceOffset(
              newValue.composing.end,
              affinity: FlarkMapAffinity.downstream,
            ),
          );
        }
      }
      _publish(
        result: _presentation.result,
        projection: _presentation.projection,
        composing: composingSource,
        forceReveal: _hasComposition(newValue.composing),
      );
    }
    urgent.stop();
    lastUrgentMicros = urgent.elapsedMicroseconds;
  }

  FlarkMarkdownParseResult _parse() {
    final stopwatch = Stopwatch()..start();
    final result = backend.parseSync(
      FlarkMarkdownParseRequest(
        revision: _source.revision,
        markdown: _activeSource,
        profile: FlarkMarkdownProfile.commonMarkGfm,
        maxSyncUtf8Bytes: 64 * 1024,
      ),
    );
    stopwatch.stop();
    parseMicros.add(stopwatch.elapsedMicroseconds);
    if (result == null) {
      throw StateError('the bounded active shard did not parse synchronously');
    }
    return result;
  }

  void _parseAndPublish({
    required TextRange composing,
    required bool forceReveal,
  }) {
    final result = _parse();
    _publish(
      result: result,
      projection: FlarkProjection.fromParseResult(result),
      composing: composing,
      forceReveal: forceReveal,
    );
  }

  void _publish({
    required FlarkMarkdownParseResult result,
    required FlarkProjection projection,
    required TextRange composing,
    required bool forceReveal,
  }) {
    _forceReveal = forceReveal;
    final revealed = mode == _FocusedSyntaxMode.reveal || _forceReveal;
    _presentation = _Presentation(
      source: _activeSource,
      result: result,
      projection: projection,
      revealed: revealed,
    );
    final selection = revealed
        ? _textSelection(_sourceSelection)
        : _textSelection(projection.sourceSelectionToDisplay(_sourceSelection));
    active.value = _ActivePayload(
      presentation: _presentation,
      value: TextEditingValue(
        text: _presentation.displayText,
        selection: selection,
        composing: revealed ? composing : TextRange.empty,
      ),
    );
  }

  void dispose() {
    active.dispose();
  }
}

final class _ActivePayload {
  const _ActivePayload({required this.presentation, required this.value});

  final _Presentation presentation;
  final TextEditingValue value;
}

final class _Presentation {
  _Presentation({
    required this.source,
    required this.result,
    required this.projection,
    required this.revealed,
  }) : displayText = revealed ? source : projection.projectText(source),
       isCodeBlock = _containsBlockKind(
         result.blocks,
         FlarkMarkdownBlockKind.codeBlock,
       ) {
    runs = _presentationRuns(this);
  }

  final String source;
  final FlarkMarkdownParseResult result;
  final FlarkProjection projection;
  final bool revealed;
  final String displayText;
  final bool isCodeBlock;
  late final List<_PresentationRun> runs;

  bool get hasStrong => result.inlineTokens.any(
    (token) => token.kind == FlarkMarkdownInlineKind.strong,
  );
}

final class _PresentationRun {
  const _PresentationRun({
    required this.start,
    required this.end,
    required this.strong,
    required this.emphasis,
    required this.code,
    required this.marker,
  });

  final int start;
  final int end;
  final bool strong;
  final bool emphasis;
  final bool code;
  final bool marker;
}

List<_PresentationRun> _presentationRuns(_Presentation presentation) {
  final intervals = <_StyleInterval>[];
  for (final token in presentation.result.inlineTokens) {
    final sourceRange = token.sourceRange;
    final start = presentation.revealed
        ? sourceRange.start
        : presentation.projection.sourceToDisplayOffset(sourceRange.start);
    final end = presentation.revealed
        ? sourceRange.end
        : presentation.projection.sourceToDisplayOffset(sourceRange.end);
    if (start == end) continue;
    intervals.add(
      _StyleInterval(
        start: start,
        end: end,
        strong: token.kind == FlarkMarkdownInlineKind.strong,
        emphasis: token.kind == FlarkMarkdownInlineKind.emphasis,
        code: token.kind == FlarkMarkdownInlineKind.inlineCode,
      ),
    );
  }
  if (presentation.revealed) {
    for (final marker in presentation.result.hiddenRanges) {
      intervals.add(
        _StyleInterval(
          start: marker.sourceRange.start,
          end: marker.sourceRange.end,
          marker: true,
        ),
      );
    }
  }

  final boundaries = <int>{0, presentation.displayText.length};
  for (final interval in intervals) {
    boundaries
      ..add(interval.start.clamp(0, presentation.displayText.length))
      ..add(interval.end.clamp(0, presentation.displayText.length));
  }
  final sorted = boundaries.toList()..sort();
  final output = <_PresentationRun>[];
  for (var index = 0; index + 1 < sorted.length; index += 1) {
    final start = sorted[index];
    final end = sorted[index + 1];
    if (start == end) continue;
    final covering = intervals.where(
      (interval) => interval.start <= start && interval.end >= end,
    );
    output.add(
      _PresentationRun(
        start: start,
        end: end,
        strong: covering.any((interval) => interval.strong),
        emphasis: covering.any((interval) => interval.emphasis),
        code:
            presentation.isCodeBlock ||
            covering.any((interval) => interval.code),
        marker: covering.any((interval) => interval.marker),
      ),
    );
  }
  return output;
}

final class _StyleInterval {
  const _StyleInterval({
    required this.start,
    required this.end,
    this.strong = false,
    this.emphasis = false,
    this.code = false,
    this.marker = false,
  });

  final int start;
  final int end;
  final bool strong;
  final bool emphasis;
  final bool code;
  final bool marker;
}

final class _ProductFeelViewport extends StatelessWidget {
  const _ProductFeelViewport({required this.model, required this.builds});

  final _ProductFeelModel model;
  final Map<int, int> builds;

  @override
  Widget build(BuildContext context) {
    return ListView.builder(
      scrollCacheExtent: const ScrollCacheExtent.pixels(0),
      itemCount: model.blockCount,
      itemBuilder: (context, index) {
        if (index == 0) {
          return _ProductFeelEditable(
            model: model,
            onBuild: () => builds[0] = (builds[0] ?? 0) + 1,
          );
        }
        return _ReadOnlyBlock(
          index: index,
          text: model.blockAt(index),
          onBuild: () => builds[index] = (builds[index] ?? 0) + 1,
        );
      },
    );
  }
}

final class _ReadOnlyBlock extends StatelessWidget {
  const _ReadOnlyBlock({
    required this.index,
    required this.text,
    required this.onBuild,
  });

  final int index;
  final String text;
  final VoidCallback onBuild;

  @override
  Widget build(BuildContext context) {
    onBuild();
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
      child: Text(
        text,
        style: TextStyle(fontSize: index.isEven ? 14 : 15, height: 1.25),
      ),
    );
  }
}

final class _ProductFeelEditable extends StatefulWidget {
  const _ProductFeelEditable({required this.model, required this.onBuild});

  final _ProductFeelModel model;
  final VoidCallback onBuild;

  @override
  State<_ProductFeelEditable> createState() => _ProductFeelEditableState();
}

final class _ProductFeelEditableState extends State<_ProductFeelEditable> {
  late final _PresentationTextController _controller;
  late final FocusNode _focusNode;
  var _synchronizing = false;
  late TextEditingValue _lastValue;

  @override
  void initState() {
    super.initState();
    final payload = widget.model.active.value!;
    _controller = _PresentationTextController(
      presentation: payload.presentation,
      value: payload.value,
    )..addListener(_handleControllerChanged);
    _lastValue = payload.value;
    _focusNode = FocusNode();
    widget.model.active.addListener(_syncFromModel);
  }

  void _syncFromModel() {
    final payload = widget.model.active.value!;
    _synchronizing = true;
    _controller
      ..presentation = payload.presentation
      ..value = payload.value;
    _lastValue = payload.value;
    _synchronizing = false;
    if (mounted) setState(() {});
  }

  void _handleControllerChanged() {
    if (_synchronizing) return;
    final next = _controller.value;
    if (next == _lastValue) return;
    final previous = _lastValue;
    _lastValue = next;
    widget.model.applyPlatformValue(previous, next);
  }

  @override
  void dispose() {
    widget.model.active.removeListener(_syncFromModel);
    _controller
      ..removeListener(_handleControllerChanged)
      ..dispose();
    _focusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    widget.onBuild();
    return RepaintBoundary(
      key: _activePaintBoundaryKey,
      child: DecoratedBox(
        key: _activeDecorationKey,
        decoration: BoxDecoration(
          color: widget.model.presentation.isCodeBlock
              ? const Color(0xFFF3F4F6)
              : const Color(0x00000000),
        ),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
          child: EditableText(
            key: _activeEditableKey,
            controller: _controller,
            focusNode: _focusNode,
            style: const TextStyle(fontSize: 16, height: 1.35),
            cursorColor: const Color(0xFF006ADC),
            backgroundCursorColor: const Color(0x00000000),
            maxLines: null,
          ),
        ),
      ),
    );
  }
}

final class _PresentationTextController extends TextEditingController {
  _PresentationTextController({
    required this.presentation,
    required TextEditingValue value,
  }) : super.fromValue(value);

  _Presentation presentation;

  @override
  TextSpan buildTextSpan({
    required BuildContext context,
    TextStyle? style,
    required bool withComposing,
  }) {
    final base = style ?? const TextStyle();
    final composing = withComposing && value.isComposingRangeValid
        ? value.composing
        : TextRange.empty;
    final boundaries = <int>{0, text.length};
    for (final run in presentation.runs) {
      boundaries
        ..add(run.start)
        ..add(run.end);
    }
    if (_hasComposition(composing)) {
      boundaries
        ..add(composing.start)
        ..add(composing.end);
    }
    final sorted = boundaries.toList()..sort();
    final children = <TextSpan>[];
    for (var index = 0; index + 1 < sorted.length; index += 1) {
      final start = sorted[index];
      final end = sorted[index + 1];
      if (start == end) continue;
      final run = presentation.runs.firstWhere(
        (candidate) => candidate.start <= start && candidate.end >= end,
        orElse: () => _PresentationRun(
          start: start,
          end: end,
          strong: false,
          emphasis: false,
          code: presentation.isCodeBlock,
          marker: false,
        ),
      );
      var effective = base;
      if (run.strong) {
        effective = effective.merge(
          const TextStyle(fontWeight: FontWeight.w700),
        );
      }
      if (run.emphasis) {
        effective = effective.merge(
          const TextStyle(fontStyle: FontStyle.italic),
        );
      }
      if (run.code) {
        effective = effective.merge(
          const TextStyle(
            fontFamily: 'monospace',
            backgroundColor: Color(0x14000000),
          ),
        );
      }
      if (run.marker) {
        effective = effective.merge(const TextStyle(color: Color(0x66808080)));
      }
      if (_hasComposition(composing) &&
          composing.start <= start &&
          composing.end >= end) {
        effective = effective.merge(
          const TextStyle(decoration: TextDecoration.underline),
        );
      }
      children.add(
        TextSpan(text: text.substring(start, end), style: effective),
      );
    }
    return TextSpan(style: base, children: children);
  }
}

final class _TextDiff {
  const _TextDiff({
    required this.start,
    required this.deletedLength,
    required this.replacement,
  });

  factory _TextDiff.between(String before, String after) {
    var prefix = 0;
    final prefixLimit = before.length < after.length
        ? before.length
        : after.length;
    while (prefix < prefixLimit &&
        before.codeUnitAt(prefix) == after.codeUnitAt(prefix)) {
      prefix += 1;
    }
    var beforeSuffix = before.length;
    var afterSuffix = after.length;
    while (beforeSuffix > prefix &&
        afterSuffix > prefix &&
        before.codeUnitAt(beforeSuffix - 1) ==
            after.codeUnitAt(afterSuffix - 1)) {
      beforeSuffix -= 1;
      afterSuffix -= 1;
    }
    return _TextDiff(
      start: prefix,
      deletedLength: beforeSuffix - prefix,
      replacement: after.substring(prefix, afterSuffix),
    );
  }

  final int start;
  final int deletedLength;
  final String replacement;
}

bool _containsBlockKind(
  List<FlarkMarkdownBlockNode> blocks,
  FlarkMarkdownBlockKind kind,
) {
  for (final block in blocks) {
    if (block.kind == kind || _containsBlockKind(block.children, kind)) {
      return true;
    }
  }
  return false;
}

FlarkSelection _flarkSelection(TextSelection selection) => FlarkSelection(
  baseOffset: selection.baseOffset,
  extentOffset: selection.extentOffset,
);

TextSelection _textSelection(FlarkSelection selection) => TextSelection(
  baseOffset: selection.baseOffset,
  extentOffset: selection.extentOffset,
);

bool _hasComposition(TextRange range) => range.isValid && !range.isCollapsed;

TextSpan _textSpan(WidgetTester tester, EditableText editable) {
  return editable.controller.buildTextSpan(
    context: tester.element(find.byKey(_activeEditableKey)),
    style: editable.style,
    withComposing: true,
  );
}

bool _containsBold(InlineSpan span) {
  if (span.style?.fontWeight == FontWeight.w700) return true;
  if (span is! TextSpan) return false;
  return span.children?.any(_containsBold) ?? false;
}

String _documentSource(int blockCount, String activeSource) {
  final output = StringBuffer()..writeln(activeSource);
  for (var index = 1; index < blockCount; index += 1) {
    output.writeln(_ordinaryBlock(index));
  }
  return output.toString();
}

String _ordinaryBlock(int index) {
  final suffix = index % 11 == 0
      ? ' with enough additional words to wrap onto another visual line'
      : '';
  return 'paragraph $index with **bold**, *emphasis*, and `code`$suffix';
}

int _percentile(List<int> values, int percentile) {
  final sorted = values..sort();
  return sorted[((sorted.length - 1) * percentile) ~/ 100];
}
