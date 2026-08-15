@Tags(<String>['benchmark'])
library;

import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark_flutter/src/v2/flutter/flutter.dart';
import 'package:flark/src/v2/markdown/markdown.dart';

void main() {
  for (final blockCount in const [1000, 5000]) {
    testWidgets('current live block editor scaling at $blockCount blocks', (
      tester,
    ) async {
      final backend = FlarkNativeComrakParseBackend.tryLoad();
      if (backend == null) {
        debugPrint(
          'flark_prototype current_live_${blockCount}blocks skipped=no_bridge',
        );
        return;
      }

      final markdown = _taskListMarkdown(blockCount);
      final controller = FlarkFlutterController.fromMarkdown(markdown);
      addTearDown(controller.dispose);
      final parsed = await tester.runAsync(
        () => backend.parse(
          FlarkMarkdownParseRequest(
            revision: controller.state.revision,
            markdown: markdown,
            profile: FlarkMarkdownProfile.commonMarkGfm,
          ),
        ),
      );
      expect(controller.applyParseResult(parsed!), isTrue);

      final initial = Stopwatch()..start();
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 600,
            height: 600,
            child: FlarkLiveRenderedEditableText(
              controller: controller,
              style: const TextStyle(fontSize: 14),
            ),
          ),
        ),
      );
      await tester.pump();
      initial.stop();

      final rendered = find.byType(EditableText).evaluate().length;
      final applySamples = <Duration>[];
      final pumpSamples = <Duration>[];
      for (var index = 0; index < 5; index += 1) {
        final apply = Stopwatch()..start();
        controller.applyTransaction(_insertAt(5));
        apply.stop();
        applySamples.add(apply.elapsed);

        final pump = Stopwatch()..start();
        await tester.pump();
        pump.stop();
        pumpSamples.add(pump.elapsed);
      }

      debugPrint(
        'flark_prototype current_live_${blockCount}blocks '
        'chars=${markdown.length} mounted_editables=$rendered '
        'initial=${_fmt(initial.elapsed)} '
        'apply_median=${_fmt(_median(applySamples))} '
        'pump_median=${_fmt(_median(pumpSamples))}',
      );
    });
  }

  for (final blockCount in const [5000, 50000]) {
    testWidgets('lazy editable block prototype at $blockCount blocks', (
      tester,
    ) async {
      final store = _PrototypeBlockStore(blockCount);
      addTearDown(store.dispose);

      final initial = Stopwatch()..start();
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 600,
            height: 600,
            child: _PrototypeLazyBlockEditor(store: store),
          ),
        ),
      );
      await tester.pump();
      initial.stop();

      final mounted = find.byType(EditableText).evaluate().length;
      final mutateSamples = <Duration>[];
      final pumpSamples = <Duration>[];
      for (var index = 0; index < 20; index += 1) {
        final mutate = Stopwatch()..start();
        store.insertInFirstBlock('x');
        mutate.stop();
        mutateSamples.add(mutate.elapsed);

        final pump = Stopwatch()..start();
        await tester.pump();
        pump.stop();
        pumpSamples.add(pump.elapsed);
      }

      debugPrint(
        'flark_prototype lazy_live_${blockCount}blocks '
        'mounted_editables=$mounted initial=${_fmt(initial.elapsed)} '
        'mutate_median=${_fmt(_median(mutateSamples))} '
        'pump_median=${_fmt(_median(pumpSamples))}',
      );

      expect(mounted, lessThan(40));
      expect(find.textContaining('xxxxxxxx'), findsOneWidget);
    });
  }
}

final class _PrototypeBlockStore extends ChangeNotifier {
  _PrototypeBlockStore(int count)
    : _blocks = List<String>.generate(
        count,
        (index) => 'task item $index with a little inline text',
        growable: false,
      );

  final List<String> _blocks;

  int get length => _blocks.length;

  String blockAt(int index) => _blocks[index];

  void insertInFirstBlock(String text) {
    _blocks[0] = _blocks[0].replaceRange(5, 5, text);
    notifyListeners();
  }
}

final class _PrototypeLazyBlockEditor extends StatelessWidget {
  const _PrototypeLazyBlockEditor({required this.store});

  final _PrototypeBlockStore store;

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: store,
      builder: (context, _) {
        return ListView.builder(
          scrollCacheExtent: const ScrollCacheExtent.pixels(0),
          itemExtent: 28,
          itemCount: store.length,
          itemBuilder: (context, index) {
            return _PrototypeEditableBlock(
              key: ValueKey(index),
              text: store.blockAt(index),
            );
          },
        );
      },
    );
  }
}

final class _PrototypeEditableBlock extends StatefulWidget {
  const _PrototypeEditableBlock({super.key, required this.text});

  final String text;

  @override
  State<_PrototypeEditableBlock> createState() =>
      _PrototypeEditableBlockState();
}

final class _PrototypeEditableBlockState
    extends State<_PrototypeEditableBlock> {
  late final TextEditingController _controller;
  late final FocusNode _focusNode;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.text);
    _focusNode = FocusNode();
  }

  @override
  void didUpdateWidget(_PrototypeEditableBlock oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.text == widget.text) return;
    _controller.value = TextEditingValue(
      text: widget.text,
      selection: TextSelection.collapsed(offset: widget.text.length),
    );
  }

  @override
  void dispose() {
    _controller.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
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

FlarkTransaction _insertAt(int offset) {
  return FlarkTransaction.single(
    FlarkSourceOperation.insert(offset, 'x'),
    metadata: const FlarkTransactionMetadata(
      intent: FlarkTransactionIntent.input,
      userEvent: 'prototype.insert',
    ),
  );
}

String _taskListMarkdown(int count) {
  final buffer = StringBuffer();
  for (var index = 0; index < count; index += 1) {
    buffer.writeln('- [ ] task item $index with a little inline text');
  }
  return buffer.toString();
}

Duration _median(List<Duration> samples) {
  samples.sort();
  return samples[samples.length ~/ 2];
}

String _fmt(Duration duration) {
  final micros = duration.inMicroseconds;
  if (micros < 1000) return '${micros}us';
  return '${(micros / 1000).toStringAsFixed(2)}ms';
}
