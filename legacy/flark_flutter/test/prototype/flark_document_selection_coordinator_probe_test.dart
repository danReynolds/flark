import 'dart:ui' show PointerDeviceKind;

import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('document gesture layer selects across lazy input shards', (
    tester,
  ) async {
    final model = _DocumentModel(50000);
    addTearDown(model.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          width: 600,
          height: 600,
          child: _DocumentEditor(model: model),
        ),
      ),
    );
    await tester.pump();

    final editables = find.byType(EditableText);
    final mounted = editables.evaluate().length;
    expect(mounted, lessThan(40));

    final first = tester
        .state<EditableTextState>(editables.at(0))
        .renderEditable;
    final tenth = tester
        .state<EditableTextState>(editables.at(10))
        .renderEditable;
    final from = first.localToGlobal(
      first.getLocalRectForCaret(const TextPosition(offset: 2)).center,
    );
    final to = tenth.localToGlobal(
      tenth.getLocalRectForCaret(const TextPosition(offset: 12)).center,
    );
    final gesture = await tester.startGesture(
      from,
      kind: PointerDeviceKind.mouse,
    );
    await tester.pump();
    await gesture.moveTo(to);
    await tester.pump();
    await gesture.up();
    await tester.pump();

    final expectedStart = model.blockStart(0) + 2;
    final expectedEnd = model.blockStart(10) + 12;
    expect(model.selection.start, expectedStart);
    expect(model.selection.end, expectedEnd);
    // ignore: avoid_print
    print(
      'flark_prototype coordinated_cross_block_drag '
      'mounted=$mounted selection=${model.selection.start}..'
      '${model.selection.end} blocks=${model.length}',
    );
  });
}

final class _DocumentModel extends ChangeNotifier {
  _DocumentModel(this.length);

  static const blockText = 'task item with a little inline text';

  final int length;
  TextSelection selection = const TextSelection.collapsed(offset: 0);

  int blockStart(int index) => index * (blockText.length + 1);

  void select(int anchor, int extent) {
    selection = TextSelection(baseOffset: anchor, extentOffset: extent);
    notifyListeners();
  }
}

final class _DocumentEditor extends StatefulWidget {
  const _DocumentEditor({required this.model});

  final _DocumentModel model;

  @override
  State<_DocumentEditor> createState() => _DocumentEditorState();
}

final class _DocumentEditorState extends State<_DocumentEditor> {
  final Map<int, RenderEditable Function()> _mounted = {};
  int? _anchor;

  @override
  Widget build(BuildContext context) {
    return Listener(
      behavior: HitTestBehavior.opaque,
      onPointerDown: (event) {
        if (event.kind != PointerDeviceKind.mouse) return;
        _anchor = _sourceOffsetAt(event.position);
      },
      onPointerMove: (event) {
        final anchor = _anchor;
        if (anchor == null) return;
        widget.model.select(anchor, _sourceOffsetAt(event.position));
      },
      onPointerUp: (event) {
        final anchor = _anchor;
        if (anchor != null) {
          widget.model.select(anchor, _sourceOffsetAt(event.position));
        }
        _anchor = null;
      },
      child: AnimatedBuilder(
        animation: widget.model,
        builder: (context, _) {
          return ListView.builder(
            scrollCacheExtent: const ScrollCacheExtent.pixels(0),
            itemExtent: 28,
            itemCount: widget.model.length,
            itemBuilder: (context, index) {
              return IgnorePointer(
                child: _InputShard(
                  key: ValueKey(index),
                  index: index,
                  text: _DocumentModel.blockText,
                  selection: _localSelection(index),
                  onMount: (renderEditable) {
                    _mounted[index] = renderEditable;
                  },
                  onUnmount: () => _mounted.remove(index),
                ),
              );
            },
          );
        },
      ),
    );
  }

  TextSelection _localSelection(int index) {
    final start = widget.model.blockStart(index);
    final end = start + _DocumentModel.blockText.length;
    final selection = widget.model.selection;
    if (selection.end < start || selection.start > end) {
      return const TextSelection.collapsed(offset: 0);
    }
    return TextSelection(
      baseOffset: (selection.baseOffset - start).clamp(0, end - start),
      extentOffset: (selection.extentOffset - start).clamp(0, end - start),
    );
  }

  int _sourceOffsetAt(Offset globalPosition) {
    if (_mounted.isEmpty) return 0;
    MapEntry<int, RenderEditable Function()>? closest;
    var closestDistance = double.infinity;
    for (final entry in _mounted.entries) {
      final render = entry.value();
      final rect = render.localToGlobal(Offset.zero) & render.size;
      if (rect.contains(globalPosition)) {
        final position = render.getPositionForPoint(globalPosition);
        return widget.model.blockStart(entry.key) + position.offset;
      }
      final distance = globalPosition.dy < rect.top
          ? rect.top - globalPosition.dy
          : globalPosition.dy - rect.bottom;
      if (distance < closestDistance) {
        closestDistance = distance;
        closest = entry;
      }
    }
    final entry = closest!;
    final render = entry.value();
    final position = render.getPositionForPoint(globalPosition);
    return widget.model.blockStart(entry.key) + position.offset;
  }
}

final class _InputShard extends StatefulWidget {
  const _InputShard({
    super.key,
    required this.index,
    required this.text,
    required this.selection,
    required this.onMount,
    required this.onUnmount,
  });

  final int index;
  final String text;
  final TextSelection selection;
  final void Function(RenderEditable Function()) onMount;
  final VoidCallback onUnmount;

  @override
  State<_InputShard> createState() => _InputShardState();
}

final class _InputShardState extends State<_InputShard> {
  late final TextEditingController _controller;
  late final FocusNode _focusNode;
  final _key = GlobalKey<EditableTextState>();

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.text);
    _focusNode = FocusNode();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) widget.onMount(() => _key.currentState!.renderEditable);
    });
  }

  @override
  void didUpdateWidget(_InputShard oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (_controller.selection != widget.selection) {
      _controller.selection = widget.selection;
    }
  }

  @override
  void dispose() {
    widget.onUnmount();
    _controller.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return EditableText(
      key: _key,
      controller: _controller,
      focusNode: _focusNode,
      style: const TextStyle(fontSize: 14),
      cursorColor: const Color(0xFF006ADC),
      backgroundCursorColor: const Color(0x00000000),
      maxLines: 1,
    );
  }
}
