import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('active-source composition keeps one engine input host', (
    tester,
  ) async {
    final key = GlobalKey<_EngineInputProbeState>();
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(body: _EngineInputProbe(key: key)),
      ),
    );
    await tester.tap(find.byKey(const Key('engine-rendered')));
    await tester.pump();
    await tester.pump();

    final state = key.currentState!;
    final editableBefore = state.editableState;
    expect(state.firstFocusedText, '**bold**');

    const composing = TextEditingValue(
      text: '**béold**',
      selection: TextSelection.collapsed(offset: 4),
      composing: TextRange(start: 3, end: 4),
    );
    state.receive(composing);
    state.requestReshape();
    await tester.pump();

    expect(identical(editableBefore, state.editableState), isTrue);
    expect(state.value, composing);
    expect(state.appliedReshapes, 0);

    state.receive(composing.copyWith(composing: TextRange.empty));
    await tester.pump();
    expect(identical(editableBefore, state.editableState), isTrue);
    expect(state.value.text, '**béold**');
    expect(state.appliedReshapes, 1);

    debugPrint(
      'flark_phase0_engine_input platform=${defaultTargetPlatform.name} '
      'exact_source_before_focus=${state.firstFocusedText} '
      'host_stable=true composing_pinned=true',
    );
  });

  testWidgets('large virtual semantics surface pages with bounded widgets', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    final key = GlobalKey<_EngineSemanticsProbeState>();
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(body: _EngineSemanticsProbe(key: key)),
      ),
    );
    await tester.pump();

    final mountedStart = find.byType(Semantics).evaluate().length;
    expect(mountedStart, lessThan(80));
    expect(find.bySemanticsLabel('Engine paragraph 0'), findsOneWidget);

    key.currentState!.jumpTo(25000);
    await tester.pumpAndSettle();

    final mountedFar = find.byType(Semantics).evaluate().length;
    expect(mountedFar, lessThan(80));
    expect(find.bySemanticsLabel('Engine paragraph 25000'), findsOneWidget);
    expect(find.bySemanticsLabel('Engine paragraph 0'), findsNothing);

    debugPrint(
      'flark_phase0_engine_semantics platform='
      '${defaultTargetPlatform.name} blocks=50000 mounted_start=$mountedStart '
      'mounted_far=$mountedFar distant_label=true',
    );
    semantics.dispose();
  });

  testWidgets('engine fonts expose shaping seams and global wrapping', (
    tester,
  ) async {
    const arabic = TextStyle(fontFamily: 'Geeza Pro', fontSize: 42);
    const latin = TextStyle(fontFamily: 'Times New Roman', fontSize: 42);
    final arabicDelta =
        (_width('سلام', arabic, TextDirection.rtl) -
                _width('سل', arabic, TextDirection.rtl) -
                _width('ام', arabic, TextDirection.rtl))
            .abs();
    final latinDelta =
        (_width('office', latin, TextDirection.ltr) -
                _width('of', latin, TextDirection.ltr) -
                _width('fice', latin, TextDirection.ltr))
            .abs();
    expect(math.max(arabicDelta, latinDelta), greaterThan(0.1));

    final text = List<String>.filled(
      500,
      'سلام عليكم كتابة عربية طويلة للاختبار ',
    ).join();
    final wholeLines = _lineCount(text, arabic, 320, TextDirection.rtl);
    var mismatches = 0;
    for (var target = 300; target < text.length - 300; target += 211) {
      final split = text.indexOf(' ', target);
      if (split < 0) break;
      final independent =
          _lineCount(
            text.substring(0, split + 1),
            arabic,
            320,
            TextDirection.rtl,
          ) +
          _lineCount(text.substring(split + 1), arabic, 320, TextDirection.rtl);
      if (independent != wholeLines) mismatches += 1;
    }
    expect(mismatches, greaterThan(0));

    debugPrint(
      'flark_phase0_engine_shaping platform='
      '${defaultTargetPlatform.name} arabic_delta=$arabicDelta '
      'latin_delta=$latinDelta wrap_mismatches=$mismatches',
    );
  });
}

final class _EngineInputProbe extends StatefulWidget {
  const _EngineInputProbe({super.key});

  @override
  State<_EngineInputProbe> createState() => _EngineInputProbeState();
}

final class _EngineInputProbeState extends State<_EngineInputProbe> {
  final _controller = TextEditingController(text: '**bold**');
  final _focusNode = FocusNode();
  final _editableKey = GlobalKey<EditableTextState>();
  bool active = false;
  int queuedReshapes = 0;
  int appliedReshapes = 0;
  String? firstFocusedText;

  TextEditingValue get value => _controller.value;
  EditableTextState get editableState => _editableKey.currentState!;

  @override
  void initState() {
    super.initState();
    _focusNode.addListener(() {
      if (_focusNode.hasFocus) firstFocusedText ??= _controller.text;
    });
  }

  void receive(TextEditingValue value) {
    editableState.updateEditingValue(value);
    if (!value.composing.isValid || value.composing.isCollapsed) {
      appliedReshapes += queuedReshapes;
      queuedReshapes = 0;
    }
  }

  void requestReshape() {
    if (_controller.value.composing.isValid &&
        !_controller.value.composing.isCollapsed) {
      queuedReshapes += 1;
    } else {
      appliedReshapes += 1;
    }
  }

  void _activateInput() {
    setState(() => active = true);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _focusNode.requestFocus();
    });
  }

  @override
  void dispose() {
    _controller.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (!active) {
      return GestureDetector(
        key: const Key('engine-rendered'),
        behavior: HitTestBehavior.opaque,
        onTap: _activateInput,
        child: const Padding(
          padding: EdgeInsets.all(24),
          child: Text('bold', style: TextStyle(fontWeight: FontWeight.bold)),
        ),
      );
    }
    return Padding(
      padding: const EdgeInsets.all(24),
      child: EditableText(
        key: _editableKey,
        controller: _controller,
        focusNode: _focusNode,
        style: const TextStyle(fontSize: 20, color: Colors.black),
        cursorColor: Colors.blue,
        backgroundCursorColor: Colors.grey,
        maxLines: null,
      ),
    );
  }
}

final class _EngineSemanticsProbe extends StatefulWidget {
  const _EngineSemanticsProbe({super.key});

  @override
  State<_EngineSemanticsProbe> createState() => _EngineSemanticsProbeState();
}

final class _EngineSemanticsProbeState extends State<_EngineSemanticsProbe> {
  static const extent = 40.0;
  final controller = ScrollController();

  void jumpTo(int index) => controller.jumpTo(index * extent);

  @override
  void dispose() {
    controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return ListView.builder(
      controller: controller,
      itemExtent: extent,
      itemCount: 50000,
      semanticChildCount: 50000,
      itemBuilder: (context, index) => Semantics(
        container: true,
        label: 'Engine paragraph $index',
        child: ExcludeSemantics(child: Text('Engine paragraph $index')),
      ),
    );
  }
}

double _width(String text, TextStyle style, TextDirection direction) {
  final painter = TextPainter(
    text: TextSpan(text: text, style: style),
    textDirection: direction,
  )..layout();
  final result = painter.width;
  painter.dispose();
  return result;
}

int _lineCount(
  String text,
  TextStyle style,
  double width,
  TextDirection direction,
) {
  final painter = TextPainter(
    text: TextSpan(text: text, style: style),
    textDirection: direction,
  )..layout(maxWidth: width);
  final result = painter.computeLineMetrics().length;
  painter.dispose();
  return result;
}
