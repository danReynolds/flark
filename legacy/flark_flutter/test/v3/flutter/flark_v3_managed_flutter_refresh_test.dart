import 'dart:typed_data';

// ignore: implementation_imports
import 'package:flark/src/v3/runtime/public/flark_v3_inline_facts.dart';
// ignore: implementation_imports
import 'package:flark/src/v3/runtime/public/flark_v3_indented_code_projection.dart'
    show FlarkV3IndentedCodeProjectionDecoder;
import 'package:flark_flutter/flark_flutter_advanced.dart';
// ignore: implementation_imports
import 'package:flark_flutter/src/v3/flutter/flark_v3_managed_flutter_binding.dart'
    show FlarkV3ManagedFlutterRefreshCoordinator;
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets(
    'exact-current transition cannot coalesce behind an identical refresh key',
    (tester) async {
      const source = '**a**';
      final hostStore = _IdleHostStore();
      final document = FlarkDocumentSession.attach(
        sourceSession: FlarkV3SourceSession.fromString(source),
        documentSession: _documentSession,
        hostStore: hostStore,
      );
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: document,
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 64,
          value: const TextEditingValue(
            text: source,
            selection: TextSelection.collapsed(offset: 3),
          ),
        ),
        queryBudget: _queryBudget,
        sourceTransactionApplier: document.apply,
        pointQueryOwnership:
            FlarkV3FlutterPointQueryOwnership.managedCoordinator,
      );
      var exactCurrent = false;
      var queries = 0;
      final coordinator = FlarkV3ManagedFlutterRefreshCoordinator(
        document: document,
        controller: controller,
        queryBudget: _queryBudget,
        isExactCurrent: () => exactCurrent,
        ensureActiveProjectionAtUtf16:
            (_, {required affinity, required query}) =>
                FlarkV3LeafProjectionDemandDisposition.notApplicable,
        inlinePresentationGeneration: () => 0,
        inlineAttemptOutcomeGeneration: () => 0,
        inlineDemandReady: () => true,
        queryAtUtf16: (positionUtf16, {required affinity, required budget}) {
          queries += 1;
          return _paragraphQuery(
            controller,
            startUtf16: 0,
            endUtf16: source.length,
            kind: 2,
            factLength: source.length,
            contentStart: 2,
          );
        },
      )..start();
      addTearDown(() {
        coordinator.dispose();
        controller.dispose();
        document.close();
      });

      expect(queries, 0);
      expect(controller.editingController.text, source);

      exactCurrent = true;
      coordinator.refresh();
      await tester.pump();

      expect(queries, 1);
      expect(controller.editingController.text, 'a');
    },
  );

  testWidgets(
    'selection refresh adopts the exact projected leaf on one input client',
    (tester) async {
      const source = '**a**\n\n_b_';
      final hostStore = _IdleHostStore();
      final document = FlarkDocumentSession.attach(
        sourceSession: FlarkV3SourceSession.fromString(source),
        documentSession: _documentSession,
        hostStore: hostStore,
      );
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: document,
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 64,
          value: const TextEditingValue(
            text: '**a**',
            selection: TextSelection.collapsed(offset: 3),
          ),
        ),
        queryBudget: _queryBudget,
        sourceTransactionApplier: document.apply,
        pointQueryOwnership:
            FlarkV3FlutterPointQueryOwnership.managedCoordinator,
      );
      final queries = <int>[];
      final affinities = <FlarkV3DocumentQueryAffinity>[];
      final coordinator = FlarkV3ManagedFlutterRefreshCoordinator(
        document: document,
        controller: controller,
        queryBudget: _queryBudget,
        isExactCurrent: () => true,
        ensureActiveProjectionAtUtf16:
            (_, {required affinity, required query}) =>
                FlarkV3LeafProjectionDemandDisposition.notApplicable,
        inlinePresentationGeneration: () => 0,
        inlineAttemptOutcomeGeneration: () => 0,
        inlineDemandReady: () => true,
        queryAtUtf16: (positionUtf16, {required affinity, required budget}) {
          queries.add(positionUtf16);
          affinities.add(affinity);
          return positionUtf16 < 7 ||
                  (positionUtf16 == 7 &&
                      affinity == FlarkV3DocumentQueryAffinity.upstream)
              ? _paragraphQuery(
                  controller,
                  startUtf16: 0,
                  endUtf16: 5,
                  kind: 2,
                  factLength: 5,
                  contentStart: 2,
                )
              : _paragraphQuery(
                  controller,
                  startUtf16: 7,
                  endUtf16: 10,
                  kind: 1,
                  factLength: 3,
                  contentStart: 1,
                );
        },
      )..start();
      addTearDown(() {
        coordinator.dispose();
        controller.dispose();
        document.close();
      });
      expect(queries, [3]);
      expect(hostStore.queryCount, 0);
      expect(controller.editingController.text, 'a');

      final editableKey = GlobalKey<EditableTextState>();
      final focusNode = FocusNode();
      addTearDown(focusNode.dispose);
      await tester.pumpWidget(
        _testApp(
          FlarkV3LiveEditorPrototype(
            controller: controller,
            editableKey: editableKey,
            focusNode: focusNode,
            paintLayerBuilder: (context, state) => const SizedBox.shrink(),
          ),
        ),
      );
      final editableState = editableKey.currentState!;
      final editingController = controller.editingController;
      editableState.requestKeyboard();
      await tester.pump();
      final initialSetClients = tester.testTextInput.log
          .where((call) => call.method == 'TextInput.setClient')
          .toList();
      final clientId =
          (initialSetClients.last.arguments as List<dynamic>).first as int;

      controller.updateLocalEditingValue(
        controller.editingController.value.copyWith(
          selection: const TextSelection.collapsed(offset: 0),
          composing: TextRange.empty,
        ),
      );
      await tester.pump();
      await tester.pump();
      expect(
        queries,
        [3],
        reason: 'caret movement strictly inside one exact leaf reuses decoding',
      );
      expect(hostStore.queryCount, 0);

      controller.updateLocalEditingValue(
        controller.editingController.value.copyWith(
          selection: const TextSelection.collapsed(offset: 1),
          composing: const TextRange(start: 0, end: 1),
        ),
      );
      await tester.pump();
      await tester.pump();
      expect(queries, [
        3,
      ], reason: 'composition movement inside the leaf reuses decoding');
      controller.updateLocalEditingValue(
        controller.editingController.value.copyWith(composing: TextRange.empty),
      );
      await tester.pump();
      await tester.pump();
      expect(queries, [3]);

      controller.handoffInputIslandToExactRange(
        startUtf16: 5,
        endUtf16: 10,
        nextGlobalEditingState: const FlarkV3GlobalEditingState(
          selection: TextSelection(baseOffset: 2, extentOffset: 7),
          composing: TextRange.empty,
        ),
      );
      await tester.pump();
      await tester.pump();

      expect(queries, [3, 7]);
      expect(affinities, [
        FlarkV3DocumentQueryAffinity.downstream,
        FlarkV3DocumentQueryAffinity.downstream,
      ]);
      expect(controller.inputIslandGlobalStartUtf16, 7);
      expect(controller.inputIslandGlobalEndUtf16, 10);
      expect(controller.editingController.text, 'b');
      expect(
        controller.editingController.selection,
        const TextSelection.collapsed(offset: 0),
      );
      expect(
        controller.globalEditingState.selection,
        const TextSelection(baseOffset: 2, extentOffset: 7),
      );
      expect(controller.editingController, same(editingController));
      expect(editableKey.currentState, same(editableState));
      expect(
        tester.testTextInput.log
            .where((call) => call.method == 'TextInput.setClient')
            .length,
        initialSetClients.length,
      );
      expect(
        (tester.testTextInput.log
                    .lastWhere((call) => call.method == 'TextInput.setClient')
                    .arguments
                as List<dynamic>)
            .first,
        clientId,
      );

      controller.handleSessionExecutorProgress();
      await tester.pump();
      expect(queries, [
        3,
        7,
      ], reason: 'presentation-only frames must not re-query a stable key');
      expect(hostStore.queryCount, 0);
    },
    timeout: const Timeout(Duration(seconds: 20)),
  );

  testWidgets(
    'selection refresh exact-handoffs structural and typed gaps as literal source',
    (tester) async {
      const source = '**a**\n\n_b_';
      final hostStore = _IdleHostStore();
      final document = FlarkDocumentSession.attach(
        sourceSession: FlarkV3SourceSession.fromString(source),
        documentSession: _documentSession,
        hostStore: hostStore,
      );
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: document,
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 64,
          value: const TextEditingValue(
            text: '**a**',
            selection: TextSelection.collapsed(offset: 3),
          ),
        ),
        queryBudget: _queryBudget,
        sourceTransactionApplier: document.apply,
        pointQueryOwnership:
            FlarkV3FlutterPointQueryOwnership.managedCoordinator,
      );
      var useTypedGap = false;
      final queries = <int>[];
      final coordinator = FlarkV3ManagedFlutterRefreshCoordinator(
        document: document,
        controller: controller,
        queryBudget: _queryBudget,
        isExactCurrent: () => true,
        ensureActiveProjectionAtUtf16:
            (_, {required affinity, required query}) =>
                FlarkV3LeafProjectionDemandDisposition.notApplicable,
        inlinePresentationGeneration: () => 0,
        inlineAttemptOutcomeGeneration: () => 0,
        inlineDemandReady: () => true,
        queryAtUtf16: (positionUtf16, {required affinity, required budget}) {
          queries.add(positionUtf16);
          if (positionUtf16 != 6) {
            return _paragraphQuery(
              controller,
              startUtf16: 0,
              endUtf16: 5,
              kind: 2,
              factLength: 5,
              contentStart: 2,
            );
          }
          return useTypedGap
              ? FlarkV3DocumentSourceGapQuery(
                  sourceRevision: controller.sourceVersion.revision,
                  structureRevision: controller.sourceVersion.revision,
                  range: _span(controller.source, 5, 7),
                  reason: FlarkV3DocumentQueryGapReason.unavailableFacts,
                )
              : _literalBlankQuery(controller, startUtf16: 5, endUtf16: 7);
        },
      )..start();
      addTearDown(() {
        coordinator.dispose();
        controller.dispose();
        document.close();
      });
      expect(queries, [3]);
      expect(hostStore.queryCount, 0);
      expect(controller.editingController.text, 'a');

      final editableKey = GlobalKey<EditableTextState>();
      final focusNode = FocusNode();
      addTearDown(focusNode.dispose);
      await tester.pumpWidget(
        _testApp(
          FlarkV3LiveEditorPrototype(
            controller: controller,
            editableKey: editableKey,
            focusNode: focusNode,
            paintLayerBuilder: (context, state) => const SizedBox.shrink(),
          ),
        ),
      );
      final editableState = editableKey.currentState!;
      editableState.requestKeyboard();
      await tester.pump();
      final initialSetClientCount = tester.testTextInput.log
          .where((call) => call.method == 'TextInput.setClient')
          .length;

      const gapEditingState = FlarkV3GlobalEditingState(
        selection: TextSelection.collapsed(offset: 6),
        composing: TextRange.empty,
      );
      controller.handoffInputIslandToExactRange(
        startUtf16: 4,
        endUtf16: 8,
        nextGlobalEditingState: gapEditingState,
      );
      await tester.pump();
      await tester.pump();

      expect(queries, [3, 6]);
      expect(controller.inputIslandGlobalStartUtf16, 5);
      expect(controller.inputIslandGlobalEndUtf16, 7);
      expect(controller.editingController.text, '\n\n');
      expect(controller.hasProjectedInlinePresentation, isFalse);
      expect(editableKey.currentState, same(editableState));

      useTypedGap = true;
      controller.handoffInputIslandToExactRange(
        startUtf16: 4,
        endUtf16: 8,
        nextGlobalEditingState: gapEditingState,
      );
      await tester.pump();
      await tester.pump();

      expect(queries, [3, 6, 6]);
      expect(controller.inputIslandGlobalStartUtf16, 5);
      expect(controller.inputIslandGlobalEndUtf16, 7);
      expect(controller.editingController.text, '\n\n');
      expect(controller.hasProjectedInlinePresentation, isFalse);
      expect(editableKey.currentState, same(editableState));
      expect(
        tester.testTextInput.log
            .where((call) => call.method == 'TextInput.setClient')
            .length,
        initialSetClientCount,
      );
      expect(hostStore.queryCount, 0);
    },
    timeout: const Timeout(Duration(seconds: 20)),
  );

  testWidgets(
    'missing facts coalesce, abort outcome retries once, and commit removes markers',
    (tester) async {
      const source = '**a**';
      final hostStore = _IdleHostStore();
      final document = FlarkDocumentSession.attach(
        sourceSession: FlarkV3SourceSession.fromString(source),
        documentSession: _documentSession,
        hostStore: hostStore,
      );
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: document,
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 64,
          value: const TextEditingValue(
            text: source,
            selection: TextSelection.collapsed(offset: 3),
          ),
        ),
        queryBudget: _queryBudget,
        sourceTransactionApplier: document.apply,
        pointQueryOwnership:
            FlarkV3FlutterPointQueryOwnership.managedCoordinator,
      );
      var inlineGeneration = 0;
      var inlineOutcomeGeneration = 0;
      var inlineDemandReady = false;
      var inlineFactsReady = false;
      var inlineFactsUnsupported = false;
      var queries = 0;
      final demands = <int>[];
      final coordinator = FlarkV3ManagedFlutterRefreshCoordinator(
        document: document,
        controller: controller,
        queryBudget: _queryBudget,
        isExactCurrent: () => true,
        inlinePresentationGeneration: () => inlineGeneration,
        inlineAttemptOutcomeGeneration: () => inlineOutcomeGeneration,
        inlineDemandReady: () => inlineDemandReady,
        ensureActiveProjectionAtUtf16:
            (positionUtf16, {required affinity, required query}) {
              demands.add(positionUtf16);
              return inlineDemandReady
                  ? FlarkV3LeafProjectionDemandDisposition.scheduled
                  : FlarkV3LeafProjectionDemandDisposition.notReady;
            },
        queryAtUtf16: (positionUtf16, {required affinity, required budget}) {
          queries += 1;
          if (inlineFactsUnsupported) {
            return _paragraphQueryWithUnsupportedFacts(
              controller,
              startUtf16: 0,
              endUtf16: source.length,
            );
          }
          return inlineFactsReady
              ? _paragraphQuery(
                  controller,
                  startUtf16: 0,
                  endUtf16: source.length,
                  kind: 2,
                  factLength: source.length,
                  contentStart: 2,
                )
              : _paragraphQueryWithoutFacts(
                  controller,
                  startUtf16: 0,
                  endUtf16: source.length,
                );
        },
      )..start();
      addTearDown(() {
        coordinator.dispose();
        controller.dispose();
        document.close();
      });

      expect(queries, 1);
      expect(demands, [3], reason: 'the typed callback exposes not-ready');
      expect(controller.editingController.text, source);
      expect(hostStore.queryCount, 0);

      coordinator.refresh();
      expect(queries, 1, reason: 'a stable absent-facts key must coalesce');
      expect(demands, [3]);

      inlineDemandReady = true;
      coordinator.refresh();
      expect(
        queries,
        2,
        reason: 'demand readiness must invalidate the decoded query cache',
      );
      expect(demands, [3, 3], reason: 'the ready transition schedules demand');

      inlineOutcomeGeneration = 1;
      coordinator.refresh();
      expect(queries, 3, reason: 'an abort outcome must invalidate the cache');
      expect(demands, [
        3,
        3,
        3,
      ], reason: 'the outcome permits one bounded retry');

      coordinator.refresh();
      expect(queries, 3);
      expect(demands, [3, 3, 3]);

      inlineFactsReady = true;
      inlineGeneration = 1;
      coordinator.refresh();
      await tester.pump();
      await tester.pump();

      expect(queries, 4);
      expect(demands, [3, 3, 3]);
      expect(controller.editingController.text, 'a');
      expect(hostStore.queryCount, 0);

      inlineFactsReady = false;
      inlineOutcomeGeneration = 2;
      coordinator.refresh();
      await tester.pump();
      await tester.pump();

      expect(queries, 5);
      expect(demands, [3, 3, 3, 3]);
      expect(
        controller.editingController.text,
        'a',
        reason:
            'an in-flight absent sidecar must not flash source markers over '
            'the mechanically exact projection',
      );

      inlineFactsUnsupported = true;
      inlineOutcomeGeneration = 3;
      coordinator.refresh();
      await tester.pump();
      await tester.pump();

      expect(queries, 6);
      expect(demands, [3, 3, 3, 3]);
      expect(
        controller.editingController.text,
        source,
        reason: 'a terminal unsupported result restores literal source',
      );
      expect(hostStore.queryCount, 0);
    },
    timeout: const Timeout(Duration(seconds: 20)),
  );

  testWidgets(
    'indented code hides certified prefixes and Enter preserves source syntax',
    (tester) async {
      const source = '    alpha\n      beta\n';
      final hostStore = _IdleHostStore();
      final document = FlarkDocumentSession.attach(
        sourceSession: FlarkV3SourceSession.fromString(source),
        documentSession: _documentSession,
        hostStore: hostStore,
      );
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: document,
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 64,
          value: const TextEditingValue(
            text: source,
            selection: TextSelection.collapsed(offset: 9),
          ),
        ),
        queryBudget: _queryBudget,
        sourceTransactionApplier: document.apply,
        pointQueryOwnership:
            FlarkV3FlutterPointQueryOwnership.managedCoordinator,
      );
      final coordinator = FlarkV3ManagedFlutterRefreshCoordinator(
        document: document,
        controller: controller,
        queryBudget: _queryBudget,
        isExactCurrent: () => true,
        queryAtUtf16: (_, {required affinity, required budget}) =>
            _indentedCodeQuery(controller),
        ensureActiveProjectionAtUtf16:
            (_, {required affinity, required query}) =>
                FlarkV3LeafProjectionDemandDisposition.notApplicable,
        inlinePresentationGeneration: () => 1,
        inlineAttemptOutcomeGeneration: () => 1,
        inlineDemandReady: () => true,
      )..start();
      addTearDown(() {
        coordinator.dispose();
        controller.dispose();
        document.close();
      });

      await tester.pump();
      expect(controller.editingController.text, 'alpha\n  beta\n');
      expect(controller.inputIslandGlobalStartUtf16, 0);
      expect(controller.inputIslandGlobalEndUtf16, source.length);

      // Keep this assertion focused on the mechanically retained lease rather
      // than asking the fake host to recertify the edited revision.
      coordinator.dispose();
      final edit = controller.applyTextEditingDelta(
        const TextEditingDeltaInsertion(
          oldText: 'alpha\n  beta\n',
          textInserted: '\n',
          insertionOffset: 5,
          selection: TextSelection.collapsed(offset: 6),
          composing: TextRange.empty,
        ),
      );

      expect(edit, isNotNull);
      expect(edit!.changed, isTrue);
      expect(document.source.toString(), '    alpha\n    \n      beta\n');
      expect(controller.editingController.text, 'alpha\n\n  beta\n');
      expect(controller.globalEditingState.selection.extentOffset, 14);
      expect(hostStore.queryCount, 0);
    },
  );

  testWidgets(
    'same-range authoritative refresh rebases the next projected edit',
    (tester) async {
      const source = '**a**';
      final hostStore = _IdleHostStore();
      final document = FlarkDocumentSession.attach(
        sourceSession: FlarkV3SourceSession.fromString(source),
        documentSession: _documentSession,
        hostStore: hostStore,
      );
      final controller = FlarkV3FlutterLiveController.attach(
        documentSession: document,
        inputIsland: FlarkV3InputIslandSnapshot(
          globalStartUtf16: 0,
          maximumUtf16: 64,
          value: const TextEditingValue(
            text: source,
            selection: TextSelection.collapsed(offset: 3),
          ),
        ),
        queryBudget: _queryBudget,
        sourceTransactionApplier: document.apply,
        pointQueryOwnership:
            FlarkV3FlutterPointQueryOwnership.managedCoordinator,
      );
      final coordinator = FlarkV3ManagedFlutterRefreshCoordinator(
        document: document,
        controller: controller,
        queryBudget: _queryBudget,
        isExactCurrent: () => true,
        ensureActiveProjectionAtUtf16:
            (_, {required affinity, required query}) =>
                FlarkV3LeafProjectionDemandDisposition.notApplicable,
        inlinePresentationGeneration: () => 0,
        inlineAttemptOutcomeGeneration: () => 0,
        inlineDemandReady: () => true,
        queryAtUtf16: (positionUtf16, {required affinity, required budget}) =>
            _paragraphQuery(
              controller,
              startUtf16: 0,
              endUtf16: 5,
              kind: 2,
              factLength: 5,
              contentStart: 2,
            ),
      )..start();
      addTearDown(() {
        coordinator.dispose();
        controller.dispose();
        document.close();
      });
      expect(controller.editingController.text, 'a');
      expect(hostStore.queryCount, 0);

      final editableKey = GlobalKey<EditableTextState>();
      final focusNode = FocusNode();
      addTearDown(focusNode.dispose);
      await tester.pumpWidget(
        _testApp(
          FlarkV3LiveEditorPrototype(
            controller: controller,
            editableKey: editableKey,
            focusNode: focusNode,
            paintLayerBuilder: (context, state) => const SizedBox.shrink(),
          ),
        ),
      );
      final editableState = editableKey.currentState!;
      final editingController = controller.editingController;
      editableState.requestKeyboard();
      await tester.pump();
      final initialSetClientCount = tester.testTextInput.log
          .where((call) => call.method == 'TextInput.setClient')
          .length;

      final external = document.apply(
        FlarkV3SourceTransaction.single(
          baseRevision: document.uiRevision,
          operation: const FlarkV3SourceEdit(
            startUtf16: 2,
            endUtf16: 3,
            replacement: 'b',
          ),
        ),
      );
      expect(external.changed, isTrue);
      expect(document.currentUiSourceCertified, isTrue);
      expect(document.uiRevision, 1);
      expect(document.source.toString(), '**b**');

      coordinator.refresh();
      await tester.pump();
      await tester.pump();
      expect(controller.inputIslandGlobalStartUtf16, 0);
      expect(controller.inputIslandGlobalEndUtf16, 5);
      expect(controller.editingController.text, 'b');
      expect(controller.editingController, same(editingController));
      expect(editableKey.currentState, same(editableState));

      final edit = controller.applyTextEditingDelta(
        const TextEditingDeltaReplacement(
          oldText: 'b',
          replacementText: 'c',
          replacedRange: TextRange(start: 0, end: 1),
          selection: TextSelection.collapsed(offset: 1),
          composing: TextRange.empty,
        ),
      );
      expect(edit, isNotNull);
      expect(edit!.changed, isTrue);
      expect(document.uiRevision, 2);
      expect(document.source.toString(), '**c**');

      await tester.pump();
      await tester.pump();
      expect(controller.editingController.text, 'c');
      expect(controller.editingController, same(editingController));
      expect(editableKey.currentState, same(editableState));
      expect(
        tester.testTextInput.log
            .where((call) => call.method == 'TextInput.setClient')
            .length,
        initialSetClientCount,
      );
      expect(hostStore.queryCount, 0);
    },
    timeout: const Timeout(Duration(seconds: 20)),
  );
}

FlarkV3DocumentStructuralQuery _indentedCodeQuery(
  FlarkV3FlutterLiveController controller,
) {
  final block = _span(controller.source, 0, controller.source.utf16Length);
  final empty = _span(controller.source, 0, 0);
  const facts = FlarkV3IndentedCodeFacts(
    deindentColumns: 4,
    hasBofBom: false,
    lineCount: 2,
    projectedUtf8Length: 13,
    projectedUtf16Length: 13,
    terminalLineEndingBytes: 1,
  );
  final records = Uint8List(
    2 * FlarkV3IndentedCodeProjectionDecoder.recordBytes,
  );
  final data = ByteData.sublistView(records);
  data
    ..setUint32(0, 0, Endian.little)
    ..setUint32(4, 10, Endian.little)
    ..setUint32(8, 4, Endian.little)
    ..setUint32(12, 5, Endian.little)
    ..setUint32(16, 0, Endian.little)
    ..setUint32(20, 10, Endian.little)
    ..setUint32(24, 11, Endian.little)
    ..setUint32(28, 4, Endian.little)
    ..setUint32(32, 6, Endian.little)
    ..setUint32(36, 0, Endian.little);
  final payload = FlarkV3IndentedCodeProjectionDecoder.decode(
    sourceDocument: controller.source,
    expectedSource: controller.sourceVersion,
    source: block,
    facts: facts,
    encodedRecords: records,
  );
  return FlarkV3DocumentStructuralQuery(
    sourceRevision: controller.sourceVersion.revision,
    structureRevision: controller.sourceVersion.revision,
    structure: FlarkV3DocumentStructure(
      kind: FlarkV3DocumentStructureKind.indentedCode,
      source: block,
      visibleSource: empty,
      referenceDefinitionCount: 0,
      indentedCode: facts,
    ),
    projection: FlarkV3DocumentProjection(
      kind: FlarkV3DocumentStructureKind.indentedCode,
      source: block,
      projectedSource: empty,
      runCount: 2,
    ),
    indentedCodeProjection: payload,
  );
}

FlarkV3DocumentStructuralQuery _paragraphQuery(
  FlarkV3FlutterLiveController controller, {
  required int startUtf16,
  required int endUtf16,
  required int kind,
  required int factLength,
  required int contentStart,
}) {
  final leaf = _span(controller.source, startUtf16, endUtf16);
  final record = Uint8List(FlarkV3InlineFactsDecoder.recordBytes);
  ByteData.sublistView(record)
    ..setUint8(0, kind)
    ..setUint8(1, 0)
    ..setUint16(2, 0, Endian.little)
    ..setUint32(4, 0, Endian.little)
    ..setUint32(8, factLength, Endian.little)
    ..setUint32(12, contentStart, Endian.little)
    ..setUint32(16, 1, Endian.little);
  final facts = FlarkV3InlineFactsDecoder.decode(
    sourceDocument: controller.source,
    expectedSource: controller.sourceVersion,
    factSource: controller.sourceVersion,
    expectedProfilePartition: 1,
    profilePartition: 1,
    expectedLeaf: leaf,
    factLeaf: leaf,
    disposition: FlarkV3InlineFactsDisposition.authoritative,
    factCount: 1,
    encodedFacts: record,
  );
  return FlarkV3DocumentStructuralQuery(
    sourceRevision: controller.sourceVersion.revision,
    structureRevision: controller.sourceVersion.revision,
    structure: FlarkV3DocumentStructure(
      kind: FlarkV3DocumentStructureKind.paragraph,
      source: leaf,
      visibleSource: leaf,
      referenceDefinitionCount: 0,
    ),
    projection: FlarkV3DocumentProjection(
      kind: FlarkV3DocumentStructureKind.paragraph,
      source: leaf,
      projectedSource: leaf,
      runCount: 1,
    ),
    inlineFacts: facts,
  );
}

FlarkV3DocumentStructuralQuery _literalBlankQuery(
  FlarkV3FlutterLiveController controller, {
  required int startUtf16,
  required int endUtf16,
}) {
  final source = _span(controller.source, startUtf16, endUtf16);
  final empty = _span(controller.source, startUtf16, startUtf16);
  return FlarkV3DocumentStructuralQuery(
    sourceRevision: controller.sourceVersion.revision,
    structureRevision: controller.sourceVersion.revision,
    structure: FlarkV3DocumentStructure(
      kind: FlarkV3DocumentStructureKind.unknown,
      source: source,
      visibleSource: empty,
      referenceDefinitionCount: 0,
      unknownReason: FlarkV3DocumentUnknownReason.blankBoundary,
    ),
    projection: FlarkV3DocumentProjection(
      kind: FlarkV3DocumentStructureKind.unknown,
      source: source,
      projectedSource: source,
      runCount: 1,
    ),
  );
}

FlarkV3DocumentStructuralQuery _paragraphQueryWithoutFacts(
  FlarkV3FlutterLiveController controller, {
  required int startUtf16,
  required int endUtf16,
}) {
  final leaf = _span(controller.source, startUtf16, endUtf16);
  return FlarkV3DocumentStructuralQuery(
    sourceRevision: controller.sourceVersion.revision,
    structureRevision: controller.sourceVersion.revision,
    structure: FlarkV3DocumentStructure(
      kind: FlarkV3DocumentStructureKind.paragraph,
      source: leaf,
      visibleSource: leaf,
      referenceDefinitionCount: 0,
    ),
    projection: FlarkV3DocumentProjection(
      kind: FlarkV3DocumentStructureKind.paragraph,
      source: leaf,
      projectedSource: leaf,
      runCount: 1,
    ),
  );
}

FlarkV3DocumentStructuralQuery _paragraphQueryWithUnsupportedFacts(
  FlarkV3FlutterLiveController controller, {
  required int startUtf16,
  required int endUtf16,
}) {
  final leaf = _span(controller.source, startUtf16, endUtf16);
  final facts = FlarkV3InlineFactsDecoder.decode(
    sourceDocument: controller.source,
    expectedSource: controller.sourceVersion,
    factSource: controller.sourceVersion,
    expectedProfilePartition: 1,
    profilePartition: 1,
    expectedLeaf: leaf,
    factLeaf: leaf,
    disposition: FlarkV3InlineFactsDisposition.unsupported,
    factCount: 0,
    encodedFacts: Uint8List(0),
  );
  return FlarkV3DocumentStructuralQuery(
    sourceRevision: controller.sourceVersion.revision,
    structureRevision: controller.sourceVersion.revision,
    structure: FlarkV3DocumentStructure(
      kind: FlarkV3DocumentStructureKind.paragraph,
      source: leaf,
      visibleSource: leaf,
      referenceDefinitionCount: 0,
    ),
    projection: FlarkV3DocumentProjection(
      kind: FlarkV3DocumentStructureKind.paragraph,
      source: leaf,
      projectedSource: leaf,
      runCount: 1,
    ),
    inlineFacts: facts,
  );
}

FlarkV3SourceSpan _span(
  FlarkV3SourceDocument source,
  int startUtf16,
  int endUtf16,
) => FlarkV3SourceSpan(
  startUtf8: source.utf16ToUtf8(startUtf16),
  endUtf8: source.utf16ToUtf8(endUtf16),
  startUtf16: startUtf16,
  endUtf16: endUtf16,
);

Widget _testApp(Widget child) => Directionality(
  textDirection: TextDirection.ltr,
  child: Center(child: SizedBox(width: 400, child: child)),
);

final class _IdleHostStore implements FlarkV3HostStore {
  bool _closed = false;
  int queryCount = 0;

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> observeSourceVersion(
    FlarkV3SourceVersion sourceVersion,
  ) => _closed ? _rejected() : _accepted();

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> beginOffer(
    FlarkV3HostOfferBegin begin,
  ) => _rejected();

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> admitPacket(
    FlarkV3HostPublicationPacket packet,
  ) => _rejected();

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> requestCommit(
    FlarkV3HostCommitRequest request,
  ) => _rejected();

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> abortOffer(FlarkV3OfferId offerId) =>
      _rejected();

  @override
  FlarkV3HostCallResult<FlarkV3HostPollOutcome> poll(
    FlarkV3HostWorkGrant grant,
  ) => const FlarkV3HostAccepted(FlarkV3HostPollPending());

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> acknowledgeDelivery(
    FlarkV3StructuralAck ack,
  ) => _rejected();

  @override
  FlarkV3HostCallResult<FlarkV3HostStoreQueryOutcome> queryStructural(
    FlarkV3HostPointQuery query,
  ) {
    queryCount += 1;
    return _rejected();
  }

  @override
  FlarkV3HostCallResult<FlarkV3HostUnit> close() {
    _closed = true;
    return _accepted();
  }
}

FlarkV3HostAccepted<FlarkV3HostUnit> _accepted() =>
    const FlarkV3HostAccepted(FlarkV3HostUnit.accepted);

FlarkV3HostRejected<T> _rejected<T>() => const FlarkV3HostRejected(
  FlarkV3HostRejection(
    FlarkV3HostRejectReason.invalid,
    'unused fake host operation',
  ),
);

final _documentSession = FlarkV3DocumentSessionId(101, 102, 103, 104);
final _queryBudget = FlarkV3HostQueryBudget(
  maxEncodedBytes: 64 * 1024,
  maxOpenDepth: 64,
  maxLeafCount: 256,
  maxTreeNodesVisited: 1024,
);
