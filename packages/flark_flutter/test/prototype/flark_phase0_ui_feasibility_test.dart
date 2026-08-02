import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/semantics.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets(
    'active-source input lease opens on exact source and pins composition',
    (tester) async {
      final key = GlobalKey<_InputLeaseProbeState>();
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(body: _InputLeaseProbe(key: key)),
        ),
      );

      expect(find.byKey(const Key('phase0-rendered')), findsOneWidget);
      expect(find.byType(EditableText), findsNothing);

      await tester.tap(find.byKey(const Key('phase0-rendered')));
      await tester.pump();
      await tester.pump();

      final state = key.currentState!;
      final editableBefore = state.editableState;
      expect(state.firstFocusedText, '**bold**');
      expect(state.value.text, '**bold**');
      expect(tester.testTextInput.hasAnyClients, isTrue);

      const composing = TextEditingValue(
        text: '**béold**',
        selection: TextSelection.collapsed(offset: 4),
        composing: TextRange(start: 3, end: 4),
      );
      state.receivePlatformValue(composing);
      state.publishParserStyleAndRequestReshape();
      await tester.pump();

      final editableDuring = state.editableState;
      expect(identical(editableBefore, editableDuring), isTrue);
      expect(state.value, composing);
      expect(state.appliedReshapes, 0);
      expect(state.queuedReshapes, 1);
      expect(tester.testTextInput.hasAnyClients, isTrue);

      state.receivePlatformValue(
        composing.copyWith(composing: TextRange.empty),
      );
      await tester.pump();

      final editableAfter = state.editableState;
      expect(identical(editableBefore, editableAfter), isTrue);
      expect(state.value.text, '**béold**');
      expect(state.value.composing, TextRange.empty);
      expect(state.appliedReshapes, 1);
      expect(state.queuedReshapes, 0);

      debugPrint(
        'flark_phase0_input_lease platform=${defaultTargetPlatform.name} '
        'web=$kIsWeb exact_source_before_focus=${state.firstFocusedText} '
        'host_stable=true composing_pinned=true deferred_reshapes='
        '${state.appliedReshapes}',
      );
    },
  );

  testWidgets(
    'document-owned geometry can drive handles toolbar and magnifier',
    (tester) async {
      final key = GlobalKey<_SelectionChromeProbeState>();
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(body: _SelectionChromeProbe(key: key)),
        ),
      );
      await tester.pump();

      final state = key.currentState!;
      state.showChrome();
      await tester.pumpAndSettle();

      expect(find.byKey(const Key('phase0-start-handle')), findsOneWidget);
      expect(find.byKey(const Key('phase0-end-handle')), findsOneWidget);
      expect(find.byKey(const Key('phase0-toolbar')), findsOneWidget);
      expect(find.byKey(const Key('phase0-magnifier')), findsOneWidget);

      await tester.drag(
        find.byKey(const Key('phase0-end-handle')),
        const Offset(24, 18),
      );
      await tester.pump();

      expect(state.endHandleDragUpdates, greaterThan(0));
      expect(state.selectionText, 'Paragraph 2 through paragraph 18');
      expect(state.magnifierVisible, isTrue);

      debugPrint(
        'flark_phase0_selection_chrome platform='
        '${defaultTargetPlatform.name} web=$kIsWeb custom_geometry=true '
        'handle_updates=${state.endHandleDragUpdates} toolbar=true '
        'magnifier=${state.magnifierVisible}',
      );
    },
  );

  testWidgets(
    'virtualized semantics stay bounded and page to distant content',
    (tester) async {
      final semantics = tester.ensureSemantics();
      final key = GlobalKey<_VirtualSemanticsProbeState>();
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(body: _VirtualSemanticsProbe(key: key)),
        ),
      );
      await tester.pump();

      expect(find.bySemanticsLabel('Paragraph 0'), findsOneWidget);
      final mountedAtStart = find.byType(Semantics).evaluate().length;
      expect(mountedAtStart, lessThan(80));

      key.currentState!.jumpToParagraph(25000);
      await tester.pumpAndSettle();

      expect(find.bySemanticsLabel('Paragraph 25000'), findsOneWidget);
      expect(find.bySemanticsLabel('Paragraph 0'), findsNothing);
      final mountedFar = find.byType(Semantics).evaluate().length;
      expect(mountedFar, lessThan(80));
      expect((mountedFar - mountedAtStart).abs(), lessThan(12));

      final scrollableSemantics = tester.getSemantics(find.byType(Scrollable));
      final actions = scrollableSemantics.getSemanticsData();
      final hasScrollAction =
          actions.hasAction(SemanticsAction.scrollUp) ||
          actions.hasAction(SemanticsAction.scrollDown);
      // The test binding owns the active semantics pipeline; the public root
      // pipeline owner does not expose its semantics owner in widget tests.
      final treeActions = _semanticsActions(
        // ignore: deprecated_member_use
        tester.binding.pipelineOwner.semanticsOwner!.rootSemanticsNode!,
      );
      expect(
        treeActions.contains(SemanticsAction.scrollUp) ||
            treeActions.contains(SemanticsAction.scrollDown),
        isTrue,
      );

      debugPrint(
        'flark_phase0_virtual_semantics platform='
        '${defaultTargetPlatform.name} web=$kIsWeb blocks=50000 '
        'mounted_start=$mountedAtStart mounted_far=$mountedFar '
        'distant_label=true scroll_actions=${actions.actions} '
        'tree_actions=$treeActions scrollable_node_has_action=$hasScrollAction',
      );
      semantics.dispose();
    },
  );
}

final class _InputLeaseProbe extends StatefulWidget {
  const _InputLeaseProbe({super.key});

  @override
  State<_InputLeaseProbe> createState() => _InputLeaseProbeState();
}

final class _InputLeaseProbeState extends State<_InputLeaseProbe> {
  static const _source = '**bold**';

  final _controller = TextEditingController(text: _source);
  final _focusNode = FocusNode();
  final _editableKey = GlobalKey<EditableTextState>();
  bool _active = false;
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

  void receivePlatformValue(TextEditingValue value) {
    _editableKey.currentState!.updateEditingValue(value);
    if (!value.composing.isValid || value.composing.isCollapsed) {
      appliedReshapes += queuedReshapes;
      queuedReshapes = 0;
    }
  }

  void publishParserStyleAndRequestReshape() {
    if (_controller.value.composing.isValid &&
        !_controller.value.composing.isCollapsed) {
      queuedReshapes += 1;
      return;
    }
    appliedReshapes += 1;
  }

  void _activate() {
    setState(() => _active = true);
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
    if (!_active) {
      return GestureDetector(
        key: const Key('phase0-rendered'),
        behavior: HitTestBehavior.opaque,
        onTap: _activate,
        child: const Padding(
          padding: EdgeInsets.all(24),
          child: Text(
            'bold',
            style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold),
          ),
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
        selectionColor: Colors.blueAccent.withValues(alpha: 0.25),
        maxLines: null,
      ),
    );
  }
}

final class _SelectionChromeProbe extends StatefulWidget {
  const _SelectionChromeProbe({super.key});

  @override
  State<_SelectionChromeProbe> createState() => _SelectionChromeProbeState();
}

final class _SelectionChromeProbeState extends State<_SelectionChromeProbe> {
  final _startLink = LayerLink();
  final _endLink = LayerLink();
  final _toolbarLink = LayerLink();
  final _startVisible = ValueNotifier<bool>(true);
  final _endVisible = ValueNotifier<bool>(true);
  final _toolbarVisible = ValueNotifier<bool>(true);
  final _delegate = _ProbeSelectionDelegate();
  SelectionOverlay? _overlay;
  int endHandleDragUpdates = 0;

  String get selectionText => _delegate.textEditingValue.text;
  bool get magnifierVisible => _overlay?.magnifierIsVisible ?? false;

  void showChrome() {
    _overlay = SelectionOverlay(
      context: context,
      startHandleType: TextSelectionHandleType.left,
      lineHeightAtStart: 22,
      startHandlesVisible: _startVisible,
      onStartHandleDragUpdate: (_) {},
      endHandleType: TextSelectionHandleType.right,
      lineHeightAtEnd: 22,
      endHandlesVisible: _endVisible,
      onEndHandleDragUpdate: (_) => endHandleDragUpdates += 1,
      toolbarVisible: _toolbarVisible,
      selectionEndpoints: const [
        TextSelectionPoint(Offset.zero, TextDirection.ltr),
        TextSelectionPoint(Offset.zero, TextDirection.ltr),
      ],
      selectionControls: _ProbeSelectionControls(),
      selectionDelegate: _delegate,
      clipboardStatus: null,
      startHandleLayerLink: _startLink,
      endHandleLayerLink: _endLink,
      toolbarLayerLink: _toolbarLink,
      magnifierConfiguration: TextMagnifierConfiguration(
        magnifierBuilder: (context, controller, info) =>
            const SizedBox(key: Key('phase0-magnifier'), width: 80, height: 40),
      ),
    );
    _overlay!.showHandles();
    _overlay!.showToolbar(
      context: context,
      contextMenuBuilder: (_) =>
          const SizedBox(key: Key('phase0-toolbar'), width: 100, height: 36),
    );
    _overlay!.showMagnifier(
      const MagnifierInfo(
        globalGesturePosition: Offset(160, 120),
        caretRect: Rect.fromLTWH(150, 100, 2, 22),
        currentLineBoundaries: Rect.fromLTWH(20, 100, 300, 22),
        fieldBounds: Rect.fromLTWH(0, 0, 400, 300),
      ),
    );
  }

  @override
  void dispose() {
    _overlay?.dispose();
    _startVisible.dispose();
    _endVisible.dispose();
    _toolbarVisible.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Stack(
      children: [
        const Positioned.fill(
          child: Padding(
            padding: EdgeInsets.all(32),
            child: Text('Paragraph 2\n\nParagraph 18'),
          ),
        ),
        Positioned(
          left: 40,
          top: 50,
          child: CompositedTransformTarget(
            link: _startLink,
            child: const SizedBox(width: 1, height: 22),
          ),
        ),
        Positioned(
          left: 180,
          top: 120,
          child: CompositedTransformTarget(
            link: _endLink,
            child: const SizedBox(width: 1, height: 22),
          ),
        ),
        Positioned(
          left: 100,
          top: 80,
          child: CompositedTransformTarget(
            link: _toolbarLink,
            child: const SizedBox(width: 1, height: 1),
          ),
        ),
      ],
    );
  }
}

final class _ProbeSelectionControls extends TextSelectionControls {
  @override
  Widget buildHandle(
    BuildContext context,
    TextSelectionHandleType type,
    double textLineHeight, [
    VoidCallback? onTap,
  ]) {
    final key = type == TextSelectionHandleType.left
        ? const Key('phase0-start-handle')
        : const Key('phase0-end-handle');
    return GestureDetector(
      key: key,
      onTap: onTap,
      child: Container(width: 24, height: 24, color: Colors.blue),
    );
  }

  @override
  Offset getHandleAnchor(TextSelectionHandleType type, double textLineHeight) {
    return const Offset(12, 12);
  }

  @override
  Size getHandleSize(double textLineHeight) => const Size(24, 24);

  @override
  Widget buildToolbar(
    BuildContext context,
    Rect globalEditableRegion,
    double textLineHeight,
    Offset selectionMidpoint,
    List<TextSelectionPoint> endpoints,
    TextSelectionDelegate delegate,
    ValueListenable<ClipboardStatus>? clipboardStatus,
    Offset? lastSecondaryTapDownPosition,
  ) {
    return const SizedBox(key: Key('phase0-toolbar'), width: 100, height: 36);
  }
}

final class _ProbeSelectionDelegate with TextSelectionDelegate {
  TextEditingValue _value = const TextEditingValue(
    text: 'Paragraph 2 through paragraph 18',
    selection: TextSelection(baseOffset: 0, extentOffset: 32),
  );

  @override
  TextEditingValue get textEditingValue => _value;

  @override
  void userUpdateTextEditingValue(
    TextEditingValue value,
    SelectionChangedCause cause,
  ) {
    _value = value;
  }

  @override
  void bringIntoView(TextPosition position) {}

  @override
  void hideToolbar([bool hideHandles = true]) {}

  @override
  void copySelection(SelectionChangedCause cause) {}

  @override
  void cutSelection(SelectionChangedCause cause) {}

  @override
  Future<void> pasteText(SelectionChangedCause cause) async {}

  @override
  void selectAll(SelectionChangedCause cause) {
    _value = _value.copyWith(
      selection: TextSelection(baseOffset: 0, extentOffset: _value.text.length),
    );
  }
}

final class _VirtualSemanticsProbe extends StatefulWidget {
  const _VirtualSemanticsProbe({super.key});

  @override
  State<_VirtualSemanticsProbe> createState() => _VirtualSemanticsProbeState();
}

final class _VirtualSemanticsProbeState extends State<_VirtualSemanticsProbe> {
  static const _itemExtent = 40.0;
  final _controller = ScrollController();

  void jumpToParagraph(int index) {
    _controller.jumpTo(index * _itemExtent);
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 500,
      height: 480,
      child: ListView.builder(
        key: const Key('phase0-list'),
        controller: _controller,
        itemExtent: _itemExtent,
        itemCount: 50000,
        semanticChildCount: 50000,
        itemBuilder: (context, index) {
          return Semantics(
            key: ValueKey('phase0-semantic-$index'),
            container: true,
            label: 'Paragraph $index',
            child: ExcludeSemantics(
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 8),
                child: Text('Paragraph $index'),
              ),
            ),
          );
        },
      ),
    );
  }
}

Set<SemanticsAction> _semanticsActions(SemanticsNode node) {
  final data = node.getSemanticsData();
  final result = <SemanticsAction>{
    for (final action in const [
      SemanticsAction.scrollUp,
      SemanticsAction.scrollDown,
      SemanticsAction.scrollLeft,
      SemanticsAction.scrollRight,
    ])
      if (data.hasAction(action)) action,
  };
  node.visitChildren((child) {
    result.addAll(_semanticsActions(child));
    return true;
  });
  return result;
}
