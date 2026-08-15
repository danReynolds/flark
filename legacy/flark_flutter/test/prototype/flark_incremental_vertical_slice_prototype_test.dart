@Tags(<String>['benchmark'])
library;

import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter/rendering.dart' show ScrollCacheExtent;
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../../../tool/parser_research/dart/incremental_delta_codec.dart';
import '../../../../tool/parser_research/dart/persistent_document.dart';

void main() {
  testWidgets(
    'revisioned parser delta updates only the active shard and preserves IME',
    (tester) async {
      final builds = <int, int>{};
      final model = _IncrementalViewportModel(50000);
      addTearDown(model.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 600,
            height: 600,
            child: _IncrementalViewport(model: model, builds: builds),
          ),
        ),
      );
      await tester.pump();

      final editables = find.byType(EditableText);
      final mounted = editables.evaluate().length;
      expect(mounted, lessThan(40));
      final untouchedBuilds = builds[1];
      final state = tester.state<EditableTextState>(editables.first);
      state.widget.focusNode.requestFocus();
      await tester.pump();
      expect(tester.testTextInput.hasAnyClients, isTrue);

      final oldText = state.widget.controller.text;
      const inserted = 'é';
      const insertionOffset = 5;
      final composingValue = TextEditingValue(
        text: oldText.replaceRange(insertionOffset, insertionOffset, inserted),
        selection: const TextSelection.collapsed(
          offset: insertionOffset + inserted.length,
        ),
        composing: const TextRange(
          start: insertionOffset,
          end: insertionOffset + inserted.length,
        ),
      );
      state.updateEditingValue(composingValue);
      await tester.pump();

      final pending = model.takePending();
      expect(pending.edit.baseRevision, 0);
      expect(pending.edit.revision, 1);
      expect(pending.edit.wireBytes, lessThan(64));
      expect(
        model.source.substring(0, model.cellAt(0).value.sourceText.length),
        composingValue.text,
      );

      final syntaxDelta = pending.complete(model);
      expect(syntaxDelta.wireBytes, lessThan(160));
      final encodedSyntaxDelta = syntaxDelta.encode();
      final decodedSyntaxDelta = _SyntaxDelta.decode(encodedSyntaxDelta);
      expect(model.applySyntaxDelta(decodedSyntaxDelta), isTrue);
      await tester.pump();

      expect(state.widget.controller.value, composingValue);
      expect(tester.testTextInput.hasAnyClients, isTrue);
      expect(
        builds[1],
        untouchedBuilds,
        reason: 'an adjacent visible shard rebuilt',
      );
      expect(builds[0], lessThanOrEqualTo(4));
      expect(model.projectionRevision, 1);
      expect(model.projectionHash32, model.source.contentHash32);
      expect(
        model.applySyntaxDelta(syntaxDelta),
        isFalse,
        reason: 'duplicate/stale parser results must be rejected',
      );

      state.updateEditingValue(
        composingValue.copyWith(composing: TextRange.empty),
      );
      await tester.pump();
      expect(state.widget.controller.value.composing, TextRange.empty);
      expect(
        model.pendingCount,
        0,
        reason: 'composition-only changes are not source edits',
      );

      final pumpSamples = <int>[];
      for (var iteration = 0; iteration < 60; iteration += 1) {
        final current = state.widget.controller.value;
        final offset = current.selection.extentOffset;
        state.updateEditingValue(
          current.copyWith(
            text: current.text.replaceRange(offset, offset, 'x'),
            selection: TextSelection.collapsed(offset: offset + 1),
          ),
        );
        final nextPending = model.takePending();
        final nextDelta = _SyntaxDelta.decode(
          nextPending.complete(model).encode(),
        );
        expect(model.applySyntaxDelta(nextDelta), isTrue);
        final stopwatch = Stopwatch()..start();
        await tester.pump();
        stopwatch.stop();
        pumpSamples.add(stopwatch.elapsedMicroseconds);
      }
      pumpSamples.sort();
      expect(tester.testTextInput.hasAnyClients, isTrue);
      expect(builds[1], untouchedBuilds);

      debugPrint(
        'flark_incremental_vertical_slice blocks=${model.length} '
        'source_bytes=${model.source.utf8Length} mounted=$mounted '
        'edit_wire_bytes=${pending.edit.wireBytes} '
        'syntax_wire_bytes=${syntaxDelta.wireBytes} '
        'pump_p50_us=${_percentile(pumpSamples, 50)} '
        'pump_p95_us=${_percentile(pumpSamples, 95)} '
        'pump_max_us=${pumpSamples.last} '
        'active_builds=${builds[0]} adjacent_builds=${builds[1]}',
      );
    },
  );

  test('50k-block delta loop stays local across sequential revisions', () {
    final model = _IncrementalViewportModel(50000);
    addTearDown(model.dispose);
    final samples = <int>[];
    final wireBytes = <int>[];

    for (var iteration = 0; iteration < 1000; iteration += 1) {
      final cell = model.cellAt(25000).value;
      final oldValue = TextEditingValue(
        text: cell.sourceText,
        selection: TextSelection.collapsed(offset: cell.sourceText.length),
      );
      final offset = 5 + (iteration % 7);
      final newValue = oldValue.copyWith(
        text: oldValue.text.replaceRange(offset, offset, 'x'),
        selection: TextSelection.collapsed(offset: offset + 1),
      );
      final stopwatch = Stopwatch()..start();
      final pending = model.applyLocalValue(25000, oldValue, newValue)!;
      final delta = pending.complete(model);
      final encoded = delta.encode();
      expect(model.applySyntaxDelta(_SyntaxDelta.decode(encoded)), isTrue);
      stopwatch.stop();
      samples.add(stopwatch.elapsedMicroseconds);
      wireBytes.add(pending.edit.wireBytes + encoded.length);
    }

    samples.sort();
    wireBytes.sort();
    expect(model.projectionRevision, 1000);
    expect(model.projectionHash32, model.source.contentHash32);
    debugPrint(
      'flark_incremental_delta_loop blocks=${model.length} cases=${samples.length} '
      'p50_us=${_percentile(samples, 50)} p95_us=${_percentile(samples, 95)} '
      'max_us=${samples.last} wire_p95=${_percentile(wireBytes, 95)} '
      'wire_max=${wireBytes.last}',
    );
  });
}

final class _IncrementalViewportModel {
  _IncrementalViewportModel(int count)
    : _source = PrototypePersistentDocument.fromString(_sourceFor(count)),
      _tree = _BlockTree.fromCells([
        for (var index = 0; index < count; index += 1)
          _BlockCell(
            _BlockPayload(
              stableId: index + 1,
              sourceText: _blockText(index),
              displayText: _blockText(index),
              syntaxRevision: 0,
            ),
          ),
      ]) {
    _projectionHash32 = _source.contentHash32;
  }

  PrototypePersistentDocument _source;
  _BlockTree _tree;
  final List<_PendingParse> _pending = [];
  var projectionRevision = 0;
  late int _projectionHash32;

  PrototypePersistentDocument get source => _source;
  int get length => _tree.length;
  int get pendingCount => _pending.length;
  int get projectionHash32 => _projectionHash32;

  _BlockCell cellAt(int index) => _tree.cellAt(index);

  _PendingParse? applyLocalValue(
    int index,
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) {
    if (oldValue.text == newValue.text) return null;
    final cell = _tree.cellAt(index);
    if (cell.value.sourceText != oldValue.text) {
      throw StateError('input shard is not based on the current source');
    }
    final diff = _textDiff(oldValue.text, newValue.text);
    final globalStart = _tree.sourceStartUtf16(index) + diff.start;
    final applied = _source.apply(
      PrototypeDocumentEdit(
        baseRevision: _source.revision,
        startUtf16: globalStart,
        endUtf16: globalStart + diff.deletedLength,
        replacement: diff.replacement,
      ),
    );
    _source = applied.document;
    cell.value = cell.value.copyWith(sourceText: newValue.text);
    _tree = _tree.replace(index, cell);
    final pending = _PendingParse(
      blockIndex: index,
      stableId: cell.value.stableId,
      edit: applied.parserDelta,
      expectedSourceText: newValue.text,
    );
    _pending.add(pending);
    return pending;
  }

  _PendingParse takePending() {
    if (_pending.isEmpty) throw StateError('no parser edit is pending');
    return _pending.removeAt(0);
  }

  bool applySyntaxDelta(_SyntaxDelta delta) {
    if (delta.baseRevision != projectionRevision ||
        delta.beforeHash32 != _projectionHash32 ||
        delta.revision != projectionRevision + 1 ||
        delta.revision > _source.revision) {
      return false;
    }
    if (delta.deleteCount != 1 || delta.inserted.length != 1) {
      throw UnimplementedError('prototype only replaces one parser leaf');
    }
    final cell = _tree.cellAt(delta.startIndex);
    final patch = delta.inserted.single;
    if (cell.value.stableId != patch.stableId ||
        cell.value.sourceUtf16Length != patch.sourceUtf16Length ||
        cell.value.sourceUtf8Length != patch.sourceUtf8Length) {
      return false;
    }
    cell.value = cell.value.copyWith(
      // An identity projection carries no text over the bridge: Dart already
      // owns the source. Non-identity leaves carry only local hidden and
      // replacement spans, from which display text is materialized on demand.
      displayText: cell.value.sourceText,
      syntaxRevision: delta.revision,
    );
    _tree = _tree.replace(delta.startIndex, cell);
    projectionRevision = delta.revision;
    _projectionHash32 = delta.afterHash32;
    _pending.removeWhere((pending) => pending.edit.revision <= delta.revision);
    return true;
  }

  void dispose() => _tree.disposeCells();
}

final class _PendingParse {
  const _PendingParse({
    required this.blockIndex,
    required this.stableId,
    required this.edit,
    required this.expectedSourceText,
  });

  final int blockIndex;
  final int stableId;
  final PrototypeParserEditDelta edit;
  final String expectedSourceText;

  _SyntaxDelta complete(_IncrementalViewportModel model) {
    final current = model.cellAt(blockIndex).value;
    if (current.stableId != stableId ||
        current.sourceText != expectedSourceText) {
      throw StateError('parser completion no longer matches its source leaf');
    }
    return _SyntaxDelta(
      baseRevision: edit.baseRevision,
      revision: edit.revision,
      beforeHash32: edit.beforeHash32,
      afterHash32: edit.afterHash32,
      startIndex: blockIndex,
      deleteCount: 1,
      inserted: [
        _ParsedBlockPatch(
          stableId: stableId,
          sourceUtf16Length: current.sourceUtf16Length,
          sourceUtf8Length: current.sourceUtf8Length,
          hiddenRanges: const [],
          replacements: const [],
        ),
      ],
    );
  }
}

final class _SyntaxDelta {
  const _SyntaxDelta({
    required this.baseRevision,
    required this.revision,
    required this.beforeHash32,
    required this.afterHash32,
    required this.startIndex,
    required this.deleteCount,
    required this.inserted,
  });

  final int baseRevision;
  final int revision;
  final int beforeHash32;
  final int afterHash32;
  final int startIndex;
  final int deleteCount;
  final List<_ParsedBlockPatch> inserted;

  Uint8List encode() {
    return PrototypeSyntaxDeltaWire(
      baseRevision: baseRevision,
      revision: revision,
      beforeHash32: beforeHash32,
      afterHash32: afterHash32,
      startIndex: startIndex,
      deleteCount: deleteCount,
      inserted: [
        for (final block in inserted)
          PrototypeParsedBlockWire(
            stableId: block.stableId,
            sourceUtf8Length: block.sourceUtf8Length,
            sourceUtf16Length: block.sourceUtf16Length,
            hiddenRanges: [
              for (final range in block.hiddenRanges)
                PrototypeLocalRangeWire(
                  startUtf16: range.start,
                  endUtf16: range.end,
                ),
            ],
            replacements: [
              for (final replacement in block.replacements)
                PrototypeLocalReplacementWire(
                  startUtf16: replacement.range.start,
                  endUtf16: replacement.range.end,
                  text: replacement.replacement,
                ),
            ],
          ),
      ],
    ).encode();
  }

  factory _SyntaxDelta.decode(Uint8List bytes) {
    final decoded = PrototypeSyntaxDeltaWire.decode(bytes);
    return _SyntaxDelta(
      baseRevision: decoded.baseRevision,
      revision: decoded.revision,
      beforeHash32: decoded.beforeHash32,
      afterHash32: decoded.afterHash32,
      startIndex: decoded.startIndex,
      deleteCount: decoded.deleteCount,
      inserted: [
        for (final block in decoded.inserted)
          _ParsedBlockPatch(
            stableId: block.stableId,
            sourceUtf8Length: block.sourceUtf8Length,
            sourceUtf16Length: block.sourceUtf16Length,
            hiddenRanges: [
              for (final range in block.hiddenRanges)
                TextRange(start: range.startUtf16, end: range.endUtf16),
            ],
            replacements: [
              for (final replacement in block.replacements)
                _LocalReplacement(
                  range: TextRange(
                    start: replacement.startUtf16,
                    end: replacement.endUtf16,
                  ),
                  replacement: replacement.text,
                ),
            ],
          ),
      ],
    );
  }

  int get wireBytes => encode().length;
}

final class _ParsedBlockPatch {
  const _ParsedBlockPatch({
    required this.stableId,
    required this.sourceUtf16Length,
    required this.sourceUtf8Length,
    required this.hiddenRanges,
    required this.replacements,
  });

  final int stableId;
  final int sourceUtf16Length;
  final int sourceUtf8Length;
  final List<TextRange> hiddenRanges;
  final List<_LocalReplacement> replacements;
}

final class _LocalReplacement {
  const _LocalReplacement({required this.range, required this.replacement});

  final TextRange range;
  final String replacement;
}

final class _BlockPayload {
  const _BlockPayload({
    required this.stableId,
    required this.sourceText,
    required this.displayText,
    required this.syntaxRevision,
  });

  final int stableId;
  final String sourceText;
  final String displayText;
  final int syntaxRevision;

  int get sourceUtf16Length => sourceText.length + 1;
  int get sourceUtf8Length => utf8.encode(sourceText).length + 1;
  int get displayUtf16Length => displayText.length;

  _BlockPayload copyWith({
    String? sourceText,
    String? displayText,
    int? syntaxRevision,
  }) {
    return _BlockPayload(
      stableId: stableId,
      sourceText: sourceText ?? this.sourceText,
      displayText: displayText ?? this.displayText,
      syntaxRevision: syntaxRevision ?? this.syntaxRevision,
    );
  }
}

final class _BlockCell extends ValueNotifier<_BlockPayload> {
  _BlockCell(super.value);
}

final class _BlockTree {
  const _BlockTree._(this.root);

  factory _BlockTree.fromCells(List<_BlockCell> cells) {
    if (cells.isEmpty) throw ArgumentError.value(cells, 'cells');
    return _BlockTree._(_buildBlockTree(cells, 0, cells.length));
  }

  final _BlockNode root;

  int get length => root.count;

  _BlockCell cellAt(int index) {
    if (index < 0 || index >= length) {
      throw RangeError.index(index, this, 'index', null, length);
    }
    return _cellAt(root, index);
  }

  int sourceStartUtf16(int index) {
    if (index < 0 || index >= length) {
      throw RangeError.index(index, this, 'index', null, length);
    }
    return _prefixUtf16(root, index);
  }

  _BlockTree replace(int index, _BlockCell cell) {
    return _BlockTree._(_replaceBlock(root, index, cell));
  }

  void disposeCells() {
    for (var index = 0; index < length; index += 1) {
      cellAt(index).dispose();
    }
  }
}

sealed class _BlockNode {
  const _BlockNode();

  int get count;
  int get sourceUtf16Length;
  int get sourceUtf8Length;
  int get displayUtf16Length;
}

final class _BlockLeaf extends _BlockNode {
  _BlockLeaf(this.cell)
    : sourceUtf16Length = cell.value.sourceUtf16Length,
      sourceUtf8Length = cell.value.sourceUtf8Length,
      displayUtf16Length = cell.value.displayUtf16Length;

  final _BlockCell cell;
  @override
  int get count => 1;
  @override
  final int sourceUtf16Length;
  @override
  final int sourceUtf8Length;
  @override
  final int displayUtf16Length;
}

final class _BlockBranch extends _BlockNode {
  _BlockBranch(this.left, this.right)
    : count = left.count + right.count,
      sourceUtf16Length = left.sourceUtf16Length + right.sourceUtf16Length,
      sourceUtf8Length = left.sourceUtf8Length + right.sourceUtf8Length,
      displayUtf16Length = left.displayUtf16Length + right.displayUtf16Length;

  final _BlockNode left;
  final _BlockNode right;
  @override
  final int count;
  @override
  final int sourceUtf16Length;
  @override
  final int sourceUtf8Length;
  @override
  final int displayUtf16Length;
}

_BlockNode _buildBlockTree(List<_BlockCell> cells, int start, int end) {
  if (end - start == 1) return _BlockLeaf(cells[start]);
  final middle = start + ((end - start) >> 1);
  return _BlockBranch(
    _buildBlockTree(cells, start, middle),
    _buildBlockTree(cells, middle, end),
  );
}

_BlockCell _cellAt(_BlockNode node, int index) {
  if (node case final _BlockLeaf leaf) return leaf.cell;
  final branch = node as _BlockBranch;
  if (index < branch.left.count) return _cellAt(branch.left, index);
  return _cellAt(branch.right, index - branch.left.count);
}

int _prefixUtf16(_BlockNode node, int index) {
  if (node case _BlockLeaf()) return 0;
  final branch = node as _BlockBranch;
  if (index < branch.left.count) return _prefixUtf16(branch.left, index);
  return branch.left.sourceUtf16Length +
      _prefixUtf16(branch.right, index - branch.left.count);
}

_BlockNode _replaceBlock(_BlockNode node, int index, _BlockCell cell) {
  if (node case _BlockLeaf()) return _BlockLeaf(cell);
  final branch = node as _BlockBranch;
  if (index < branch.left.count) {
    return _BlockBranch(_replaceBlock(branch.left, index, cell), branch.right);
  }
  return _BlockBranch(
    branch.left,
    _replaceBlock(branch.right, index - branch.left.count, cell),
  );
}

final class _TextDiff {
  const _TextDiff({
    required this.start,
    required this.deletedLength,
    required this.replacement,
  });

  final int start;
  final int deletedLength;
  final String replacement;
}

_TextDiff _textDiff(String oldText, String newText) {
  var prefix = 0;
  while (prefix < oldText.length &&
      prefix < newText.length &&
      oldText.codeUnitAt(prefix) == newText.codeUnitAt(prefix)) {
    prefix += 1;
  }
  var oldSuffix = oldText.length;
  var newSuffix = newText.length;
  while (oldSuffix > prefix &&
      newSuffix > prefix &&
      oldText.codeUnitAt(oldSuffix - 1) == newText.codeUnitAt(newSuffix - 1)) {
    oldSuffix -= 1;
    newSuffix -= 1;
  }
  return _TextDiff(
    start: prefix,
    deletedLength: oldSuffix - prefix,
    replacement: newText.substring(prefix, newSuffix),
  );
}

final class _IncrementalViewport extends StatelessWidget {
  const _IncrementalViewport({required this.model, required this.builds});

  final _IncrementalViewportModel model;
  final Map<int, int> builds;

  @override
  Widget build(BuildContext context) {
    return ListView.builder(
      scrollCacheExtent: const ScrollCacheExtent.pixels(0),
      itemExtent: 28,
      itemCount: model.length,
      itemBuilder: (context, index) {
        final cell = model.cellAt(index);
        return ValueListenableBuilder<_BlockPayload>(
          valueListenable: cell,
          builder: (context, payload, _) {
            return _EditableShard(
              key: ValueKey(payload.stableId),
              payload: payload,
              onValueChanged: (oldValue, newValue) {
                model.applyLocalValue(index, oldValue, newValue);
              },
              onBuild: () => builds[index] = (builds[index] ?? 0) + 1,
            );
          },
        );
      },
    );
  }
}

final class _EditableShard extends StatefulWidget {
  const _EditableShard({
    super.key,
    required this.payload,
    required this.onValueChanged,
    required this.onBuild,
  });

  final _BlockPayload payload;
  final void Function(TextEditingValue oldValue, TextEditingValue newValue)
  onValueChanged;
  final VoidCallback onBuild;

  @override
  State<_EditableShard> createState() => _EditableShardState();
}

final class _EditableShardState extends State<_EditableShard> {
  late final TextEditingController _controller;
  late final FocusNode _focusNode;
  late TextEditingValue _lastValue;
  var _synchronizing = false;

  @override
  void initState() {
    super.initState();
    _lastValue = TextEditingValue(
      text: widget.payload.sourceText,
      selection: const TextSelection.collapsed(offset: 0),
    );
    _controller = TextEditingController.fromValue(_lastValue)
      ..addListener(_handleControllerChanged);
    _focusNode = FocusNode();
  }

  @override
  void didUpdateWidget(_EditableShard oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.payload.sourceText == _controller.text) return;
    if (_focusNode.hasFocus || _controller.value.composing.isValid) return;
    _synchronizing = true;
    _lastValue = TextEditingValue(
      text: widget.payload.sourceText,
      selection: TextSelection.collapsed(
        offset: widget.payload.sourceText.length,
      ),
    );
    _controller.value = _lastValue;
    _synchronizing = false;
  }

  void _handleControllerChanged() {
    if (_synchronizing) return;
    final next = _controller.value;
    if (next == _lastValue) return;
    final previous = _lastValue;
    _lastValue = next;
    widget.onValueChanged(previous, next);
  }

  @override
  void dispose() {
    _controller.removeListener(_handleControllerChanged);
    _controller.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    widget.onBuild();
    return EditableText(
      controller: _controller,
      focusNode: _focusNode,
      style: const TextStyle(fontSize: 14),
      cursorColor: const Color(0xFF006ADC),
      backgroundCursorColor: const Color(0x00000000),
      maxLines: 1,
    );
  }
}

String _blockText(int index) => 'task item $index with a little inline text';

String _sourceFor(int count) {
  final output = StringBuffer();
  for (var index = 0; index < count; index += 1) {
    output.writeln(_blockText(index));
  }
  return output.toString();
}

int _percentile(List<int> values, int percentile) =>
    values[((values.length - 1) * percentile) ~/ 100];
