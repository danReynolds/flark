import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/v2/core/core.dart';
import 'package:sovereign_editor/src/v2/flutter/flutter.dart';
import 'package:sovereign_editor/src/v2/markdown/markdown.dart';

void main() {
  group('SovereignParseScheduler', () {
    test(
      'parses the current controller revision and applies fresh results',
      () async {
        final controller = SovereignFlutterController.fromMarkdown('hello');
        addTearDown(controller.dispose);
        final backend = _FakeParseBackend();
        final scheduler = SovereignParseScheduler(
          controller: controller,
          backend: backend,
          debounce: Duration.zero,
        )..start();
        addTearDown(scheduler.dispose);

        await _pumpMicrotasks();
        expect(backend.requests.single.markdown, 'hello');

        backend.complete(0);
        await _pumpMicrotasks();

        expect(controller.hasAuthoritativeRenderPlan, isTrue);
        expect(
          controller.renderPlan.blocks.single.sourceRange,
          const SovereignSourceRange(0, 5),
        );
      },
    );

    test(
      'reparses latest revision after stale in-flight parse completes',
      () async {
        final controller = SovereignFlutterController.fromMarkdown('a');
        addTearDown(controller.dispose);
        final backend = _FakeParseBackend();
        final scheduler = SovereignParseScheduler(
          controller: controller,
          backend: backend,
          debounce: Duration.zero,
        )..start();
        addTearDown(scheduler.dispose);

        await _pumpMicrotasks();
        expect(backend.requests.single.revision, controller.state.revision);

        controller.applyTransaction(
          SovereignTransaction.single(SovereignSourceOperation.insert(1, '!')),
        );

        backend.complete(0);
        await _pumpMicrotasks();

        expect(backend.requests.length, 2);
        expect(backend.requests.last.markdown, 'a!');
        backend.complete(1);
        await _pumpMicrotasks();

        expect(controller.hasAuthoritativeRenderPlan, isTrue);
        expect(
          controller.renderPlan.metadata['revision'],
          controller.state.revision,
        );
      },
    );

    test(
      'reports scheduled parse failures and recovers on later revisions',
      () async {
        final controller = SovereignFlutterController.fromMarkdown('a');
        addTearDown(controller.dispose);
        final backend = _FakeParseBackend();
        final errors = <Object>[];
        final scheduler = SovereignParseScheduler(
          controller: controller,
          backend: backend,
          debounce: Duration.zero,
          onError: (error, stackTrace) {
            errors.add(error);
          },
        )..start();
        addTearDown(scheduler.dispose);

        await _pumpMicrotasks();
        backend.fail(0, StateError('parse failed'));
        await _pumpMicrotasks();

        expect(errors.single, isA<StateError>());
        expect(controller.hasAuthoritativeRenderPlan, isFalse);

        controller.applyTransaction(
          SovereignTransaction.single(SovereignSourceOperation.insert(1, '!')),
        );
        await _pumpMicrotasks();

        expect(backend.requests, hasLength(2));
        expect(backend.requests.last.markdown, 'a!');
        backend.complete(1);
        await _pumpMicrotasks();

        expect(controller.hasAuthoritativeRenderPlan, isTrue);
      },
    );

    test('parseNow surfaces parser failures to awaiters', () async {
      final controller = SovereignFlutterController.fromMarkdown('a');
      addTearDown(controller.dispose);
      final backend = _FakeParseBackend();
      final scheduler = SovereignParseScheduler(
        controller: controller,
        backend: backend,
        debounce: Duration.zero,
      );
      addTearDown(scheduler.dispose);

      final parse = scheduler.parseNow();
      await _pumpMicrotasks();

      expect(backend.requests.single.markdown, 'a');
      final expectation = expectLater(parse, throwsA(isA<StateError>()));
      backend.fail(0, StateError('parse failed'));
      await expectation;

      expect(controller.hasAuthoritativeRenderPlan, isFalse);
    });
  });
}

final class _FakeParseBackend implements SovereignMarkdownParseBackend {
  final requests = <SovereignMarkdownParseRequest>[];
  final _completers = <Completer<SovereignMarkdownParseResult>>[];

  @override
  SovereignMarkdownParserCapabilities get capabilities =>
      SovereignMarkdownParserCapabilities(
        parserName: 'fake',
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        supportedProfiles: const [SovereignMarkdownProfile.commonMarkGfm],
      );

  @override
  Future<SovereignMarkdownParseResult> parse(
    SovereignMarkdownParseRequest request,
  ) {
    requests.add(request);
    final completer = Completer<SovereignMarkdownParseResult>();
    _completers.add(completer);
    return completer.future;
  }

  void complete(int index) {
    final request = requests[index];
    _completers[index].complete(
      SovereignMarkdownParseResult(
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        revision: request.revision,
        sourceTextLength: request.markdown.length,
        blocks: [
          SovereignMarkdownBlockNode(
            kind: SovereignMarkdownBlockKind.paragraph,
            type: 'paragraph',
            sourceRange: SovereignSourceRange(0, request.markdown.length),
          ),
        ],
        inlineTokens: const [],
      ),
    );
  }

  void fail(int index, Object error) {
    _completers[index].completeError(error);
  }
}

Future<void> _pumpMicrotasks() async {
  await Future<void>.delayed(Duration.zero);
  await Future<void>.delayed(Duration.zero);
}
