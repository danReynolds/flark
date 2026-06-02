import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flutter.dart';
import 'package:flark/src/v2/markdown/markdown.dart';

void main() {
  group('FlarkParseScheduler', () {
    test(
      'parses the current controller revision and applies fresh results',
      () async {
        final controller = FlarkFlutterController.fromMarkdown('hello');
        addTearDown(controller.dispose);
        final backend = _FakeParseBackend();
        final scheduler = FlarkParseScheduler(
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
          const FlarkSourceRange(0, 5),
        );
      },
    );

    test(
      'reparses latest revision after stale in-flight parse completes',
      () async {
        final controller = FlarkFlutterController.fromMarkdown('a');
        addTearDown(controller.dispose);
        final backend = _FakeParseBackend();
        final scheduler = FlarkParseScheduler(
          controller: controller,
          backend: backend,
          debounce: Duration.zero,
        )..start();
        addTearDown(scheduler.dispose);

        await _pumpMicrotasks();
        expect(backend.requests.single.revision, controller.state.revision);

        controller.applyTransaction(
          FlarkTransaction.single(FlarkSourceOperation.insert(1, '!')),
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
        final controller = FlarkFlutterController.fromMarkdown('a');
        addTearDown(controller.dispose);
        final backend = _FakeParseBackend();
        final errors = <Object>[];
        final scheduler = FlarkParseScheduler(
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
          FlarkTransaction.single(FlarkSourceOperation.insert(1, '!')),
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
      final controller = FlarkFlutterController.fromMarkdown('a');
      addTearDown(controller.dispose);
      final backend = _FakeParseBackend();
      final scheduler = FlarkParseScheduler(
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

final class _FakeParseBackend implements FlarkMarkdownParseBackend {
  final requests = <FlarkMarkdownParseRequest>[];
  final _completers = <Completer<FlarkMarkdownParseResult>>[];

  @override
  FlarkMarkdownParserCapabilities get capabilities =>
      FlarkMarkdownParserCapabilities(
        parserName: 'fake',
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        supportedProfiles: const [FlarkMarkdownProfile.commonMarkGfm],
      );

  @override
  Future<FlarkMarkdownParseResult> parse(FlarkMarkdownParseRequest request) {
    requests.add(request);
    final completer = Completer<FlarkMarkdownParseResult>();
    _completers.add(completer);
    return completer.future;
  }

  void complete(int index) {
    final request = requests[index];
    _completers[index].complete(
      FlarkMarkdownParseResult(
        schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
        revision: request.revision,
        sourceTextLength: request.markdown.length,
        blocks: [
          FlarkMarkdownBlockNode(
            kind: FlarkMarkdownBlockKind.paragraph,
            type: 'paragraph',
            sourceRange: FlarkSourceRange(0, request.markdown.length),
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
