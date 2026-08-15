@TestOn('browser')
library;

import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/flark_v3_managed_runtime_test_platform.dart';

void main() {
  testWidgets(
    'direct-media label edit survives later reference definitions on one client',
    (tester) async {
      const originalLabel = 'Flark architecture notes';
      const editedLabel = 'Flark design notes';
      const imageAlt = 'Local architecture preview';
      const linkDestination = 'https://flark.dev/revision-7';
      const linkTitle = 'Revision 7';
      const imageDestination = 'asset://checkpoint/architecture';
      const imageTitle = 'Placeholder only';
      const originalLink = '[$originalLabel]($linkDestination "$linkTitle")';
      const editedLink = '[$editedLabel]($linkDestination "$linkTitle")';
      const directParagraph =
          'Read the $originalLink beside a '
          '![$imageAlt]($imageDestination "$imageTitle").';
      const referenceParagraph =
          'Later references remain global: [launch][launch notes] and '
          '![reference diagram][reference image].';
      const source =
          '$directParagraph\n\n'
          '$referenceParagraph\n\n'
          '[launch notes]: https://flark.dev/launch "Launch notes"\n'
          '[reference image]: asset://checkpoint/reference "Reference image"\n';
      const initialDisplay = 'Read the $originalLabel beside a $imageAlt.\n';
      const editedDisplay = 'Read the $editedLabel beside a $imageAlt.\n';

      final harness = await _DirectMediaHarness.mount(
        tester,
        source: source,
        caretUtf16: source.indexOf(originalLabel) + originalLabel.length ~/ 2,
      );
      final initial = await harness.waitForExactDirectMedia(
        tester,
        containingSource: originalLink,
        expectedDisplay: initialDisplay,
      );
      _expectExactDirectMediaFacts(
        initial,
        runtime: harness.runtime,
        linkDestination: linkDestination,
        linkTitle: linkTitle,
        imageDestination: imageDestination,
        imageTitle: imageTitle,
      );
      expect(harness.runtime.status.state, FlarkV3DocumentRuntimeState.open);
      expect(harness.binding.controller.hasCertifiedInlinePresentation, isTrue);
      expect(
        harness.binding.controller.editingController.text,
        isNot(anyOf(contains(']('), contains(linkDestination))),
      );

      final editableState = harness.editableState;
      final editingController = harness.binding.controller.editingController;
      editableState.requestKeyboard();
      await tester.pump();
      final setClientCallsBefore = _setClientCalls(tester);
      final clientId =
          (setClientCallsBefore.last.arguments as List<dynamic>).first as int;
      final revisionBefore = harness.runtime.sourceRevision;
      final labelStart = editingController.text.indexOf(originalLabel);
      expect(labelStart, greaterThanOrEqualTo(0));

      (editableState as DeltaTextInputClient).updateEditingValueWithDeltas([
        TextEditingDeltaReplacement(
          oldText: editingController.text,
          replacedRange: TextRange(
            start: labelStart,
            end: labelStart + originalLabel.length,
          ),
          replacementText: editedLabel,
          selection: TextSelection.collapsed(
            offset: labelStart + editedLabel.length,
          ),
          composing: TextRange.empty,
        ),
      ]);
      await tester.pump();
      _throwPendingFlutterException(tester, 'the direct-link label edit');

      expect(harness.runtime.status.state, FlarkV3DocumentRuntimeState.open);
      expect(harness.runtime.sourceRevision, revisionBefore + 1);
      expect(
        harness.runtime.exportMarkdown(),
        source.replaceFirst(originalLink, editedLink),
      );
      expect(editingController.text, editedDisplay);

      final recertified = await harness.waitForExactDirectMedia(
        tester,
        containingSource: editedLink,
        expectedDisplay: editedDisplay,
      );
      _expectExactDirectMediaFacts(
        recertified,
        runtime: harness.runtime,
        linkDestination: linkDestination,
        linkTitle: linkTitle,
        imageDestination: imageDestination,
        imageTitle: imageTitle,
      );
      expect(harness.runtime.status.state, FlarkV3DocumentRuntimeState.open);
      expect(harness.runtime.status.sourceCurrent, isTrue);
      expect(harness.runtime.status.structureCurrent, isTrue);
      expect(harness.binding.controller.hasCertifiedInlinePresentation, isTrue);
      expect(
        harness.binding.controller.editingController,
        same(editingController),
      );
      expect(harness.editableKey.currentState, same(editableState));
      expect(harness.focusNode.hasFocus, isTrue);
      expect(tester.testTextInput.hasAnyClients, isTrue);
      final setClientCallsAfter = _setClientCalls(tester);
      expect(setClientCallsAfter, hasLength(setClientCallsBefore.length));
      expect(
        (setClientCallsAfter.last.arguments as List<dynamic>).first,
        clientId,
      );
    },
    timeout: const Timeout(Duration(seconds: 30)),
  );
}

final class _DirectMediaHarness {
  _DirectMediaHarness._({
    required this.runtime,
    required this.binding,
    required this.editableKey,
    required this.focusNode,
  });

  static Future<_DirectMediaHarness> mount(
    WidgetTester tester, {
    required String source,
    required int caretUtf16,
  }) async {
    final runtime = await runManagedRuntimeAsyncForTest(
      tester,
      () => openManagedRuntimeForTest(source),
    );
    final binding = FlarkV3ManagedFlutterBinding.attach(
      runtime: runtime,
      inputIsland: FlarkV3InputIslandSnapshot(
        globalStartUtf16: 0,
        maximumUtf16: 1024,
        value: TextEditingValue(
          text: source,
          selection: TextSelection.collapsed(offset: caretUtf16),
        ),
      ),
      queryBudget: FlarkV3HostQueryBudget(
        maxEncodedBytes: 16 * 1024,
        maxOpenDepth: 64,
        maxLeafCount: 256,
        maxTreeNodesVisited: 1024,
      ),
    );
    final editableKey = GlobalKey<EditableTextState>();
    final focusNode = FocusNode();
    final harness = _DirectMediaHarness._(
      runtime: runtime,
      binding: binding,
      editableKey: editableKey,
      focusNode: focusNode,
    );
    addTearDown(() async {
      binding.dispose();
      focusNode.dispose();
      if (runtime.status.state != FlarkV3DocumentRuntimeState.closed) {
        await runManagedRuntimeAsyncForTest(
          tester,
          () => runtime.close().timeout(const Duration(seconds: 5)),
        );
      }
    });
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkV3LiveEditorPrototype(
          controller: binding.controller,
          editableKey: editableKey,
          focusNode: focusNode,
          paintLayerBuilder: (context, state) => const SizedBox.shrink(),
        ),
      ),
    );
    _throwPendingFlutterException(tester, 'mounting the live editor');
    await runManagedRuntimeAsyncForTest(
      tester,
      () => runtime.initialReady.timeout(const Duration(seconds: 10)),
    );
    _throwPendingFlutterException(tester, 'opening the managed runtime');
    return harness;
  }

  final FlarkV3DocumentRuntime runtime;
  final FlarkV3ManagedFlutterBinding binding;
  final GlobalKey<EditableTextState> editableKey;
  final FocusNode focusNode;

  EditableTextState get editableState => editableKey.currentState!;

  Future<FlarkV3DocumentStructuralQuery> waitForExactDirectMedia(
    WidgetTester tester, {
    required String containingSource,
    required String expectedDisplay,
  }) async {
    final stopwatch = Stopwatch()..start();
    while (stopwatch.elapsed < const Duration(seconds: 15)) {
      final state = runtime.status.state;
      if (state == FlarkV3DocumentRuntimeState.closed ||
          state == FlarkV3DocumentRuntimeState.faulted) {
        final debugState = _debugState();
        await runManagedRuntimeAsyncForTest(
          tester,
          () => runtime.close().timeout(const Duration(seconds: 5)),
        );
        throw TestFailure(
          'Runtime closed without a causal terminal error before '
          'direct-media recertification: $debugState',
        );
      }
      await tester.pump(const Duration(milliseconds: 1));
      _throwPendingFlutterException(tester, 'waiting for parser authority');
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 1)),
      );
      final query = binding.controller.paintState.documentQuery;
      if (runtime.status.sourceCurrent &&
          runtime.status.structureCurrent &&
          query is FlarkV3DocumentStructuralQuery &&
          query.structure.kind == FlarkV3DocumentStructureKind.paragraph &&
          query.inlineFacts != null &&
          binding.controller.hasCertifiedInlinePresentation &&
          binding.controller.editingController.text == expectedDisplay) {
        final projectedSource = runtime.readSourceRange(
          query.projection.projectedSource.startUtf16,
          query.projection.projectedSource.endUtf16,
        );
        if (projectedSource.contains(containingSource)) return query;
      }
    }
    throw TestFailure(
      'Timed out waiting for exact direct-media authority: ${_debugState()}',
    );
  }

  String _debugState() {
    final status = runtime.status;
    return 'state=${status.state.name}, revision=${status.sourceRevision}, '
        'certified=${status.certifiedSourceRevision}, '
        'sourceCurrent=${status.sourceCurrent}, '
        'structureCurrent=${status.structureCurrent}, '
        'inline=${status.inlinePresentationGeneration}/'
        '${status.inlineAttemptOutcomeGeneration}, '
        'paint=${binding.controller.paintState.mode.name}, '
        'query=${binding.controller.paintState.documentQuery.runtimeType}, '
        'text=${binding.controller.editingController.text}';
  }
}

void _throwPendingFlutterException(WidgetTester tester, String phase) {
  final exception = tester.takeException();
  if (exception != null) {
    throw TestFailure('Flutter exception while $phase: $exception');
  }
}

void _expectExactDirectMediaFacts(
  FlarkV3DocumentStructuralQuery query, {
  required FlarkV3DocumentRuntime runtime,
  required String linkDestination,
  required String linkTitle,
  required String imageDestination,
  required String imageTitle,
}) {
  final facts = query.inlineFacts!;
  expect(facts.disposition, FlarkV3InlineFactsDisposition.authoritative);
  expect(facts.sourceVersion.revision, runtime.sourceRevision);
  final link = facts.facts.singleWhere(
    (fact) => fact.kind == FlarkV3InlineFactKind.directLink,
  );
  final image = facts.facts.singleWhere(
    (fact) => fact.kind == FlarkV3InlineFactKind.directImage,
  );
  expect(link.linkAnnotation?.kind, FlarkV3InlineLinkKind.direct);
  expect(link.linkAnnotation?.destination, linkDestination);
  expect(link.linkAnnotation?.title, linkTitle);
  expect(image.imageAnnotation?.destination, imageDestination);
  expect(image.imageAnnotation?.title, imageTitle);
}

List<MethodCall> _setClientCalls(WidgetTester tester) => tester
    .testTextInput
    .log
    .where((call) => call.method == 'TextInput.setClient')
    .toList(growable: false);
