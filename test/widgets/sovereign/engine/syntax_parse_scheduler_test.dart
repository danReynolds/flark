import 'dart:async';

import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/engine/syntax_engine.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_parse_scheduler.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/syntax_snapshot.dart';

void main() {
  group('SyntaxParseScheduler', () {
    test(
      'drops stale in-flight result and daisy-chains latest pending request',
      () async {
        final started = <SyntaxParseRequest>[];
        final completers = <int, Completer<SyntaxSnapshot>>{};
        final received = <int>[];

        final scheduler = SyntaxParseScheduler(
          runParse: (request) {
            started.add(request);
            return (completers[request.revision] ??=
                    Completer<SyntaxSnapshot>())
                .future;
          },
          onSnapshot: (snapshot) => received.add(snapshot.revision),
        );
        addTearDown(scheduler.dispose);

        scheduler.schedule(const SyntaxParseRequest(revision: 1, text: 'a'));
        scheduler.schedule(const SyntaxParseRequest(revision: 2, text: 'ab'));

        expect(started.map((r) => r.revision), [1]);
        expect(scheduler.pendingRequest?.revision, 2);
        expect(scheduler.pendingCount, 1);

        completers[1]!.complete(_snapshotFor(revision: 1));
        await _drainAsyncQueue();

        expect(scheduler.staleDropCount, 1);
        expect(started.map((r) => r.revision), [1, 2]);

        completers[2]!.complete(_snapshotFor(revision: 2));
        await _drainAsyncQueue();

        expect(received, [2]);
      },
    );

    test(
      'replaces pending request with latest revision while in-flight',
      () async {
        final started = <SyntaxParseRequest>[];
        final completers = <int, Completer<SyntaxSnapshot>>{};
        final received = <int>[];

        final scheduler = SyntaxParseScheduler(
          runParse: (request) {
            started.add(request);
            return (completers[request.revision] ??=
                    Completer<SyntaxSnapshot>())
                .future;
          },
          onSnapshot: (snapshot) => received.add(snapshot.revision),
        );
        addTearDown(scheduler.dispose);

        scheduler.schedule(const SyntaxParseRequest(revision: 1, text: 'a'));
        scheduler.schedule(const SyntaxParseRequest(revision: 2, text: 'ab'));
        scheduler.schedule(const SyntaxParseRequest(revision: 3, text: 'abc'));

        expect(scheduler.pendingReplaceCount, 1);
        expect(scheduler.pendingRequest?.revision, 3);
        expect(started.map((r) => r.revision), [1]);

        completers[1]!.complete(_snapshotFor(revision: 1));
        await _drainAsyncQueue();

        expect(scheduler.staleDropCount, 1);
        expect(started.map((r) => r.revision), [1, 3]);

        completers[3]!.complete(_snapshotFor(revision: 3));
        await _drainAsyncQueue();

        expect(received, [3]);
      },
    );

    test(
      'does not dispatch duplicate pending request after in-flight completion',
      () async {
        final started = <SyntaxParseRequest>[];
        final completers = <int, Completer<SyntaxSnapshot>>{};
        final received = <int>[];

        final scheduler = SyntaxParseScheduler(
          runParse: (request) {
            started.add(request);
            return (completers[request.revision] ??=
                    Completer<SyntaxSnapshot>())
                .future;
          },
          onSnapshot: (snapshot) => received.add(snapshot.revision),
        );
        addTearDown(scheduler.dispose);

        const request = SyntaxParseRequest(revision: 7, text: 'same');
        scheduler.schedule(request);
        scheduler.schedule(request);

        expect(started.map((r) => r.revision), [7]);
        expect(scheduler.pendingCount, 1);

        completers[7]!.complete(_snapshotFor(revision: 7));
        await _drainAsyncQueue();

        expect(started.map((r) => r.revision), [7]);
        expect(received, [7]);
        expect(scheduler.pendingCount, 0);
      },
    );

    test('dispatches pending request when markdown profile changes', () async {
      final started = <SyntaxParseRequest>[];
      final completers = <MarkdownSyntaxProfile, Completer<SyntaxSnapshot>>{};
      final received = <int>[];

      final scheduler = SyntaxParseScheduler(
        runParse: (request) {
          started.add(request);
          return (completers[request.profile] ??= Completer<SyntaxSnapshot>())
              .future;
        },
        onSnapshot: (snapshot) => received.add(snapshot.revision),
      );
      addTearDown(scheduler.dispose);

      const core = SyntaxParseRequest(
        revision: 9,
        text: 'same',
        profile: MarkdownSyntaxProfile.commonMarkCore,
      );
      const gfm = SyntaxParseRequest(
        revision: 9,
        text: 'same',
        profile: MarkdownSyntaxProfile.commonMarkGfm,
      );

      scheduler.schedule(core);
      scheduler.schedule(gfm);

      expect(started, [core]);
      expect(scheduler.pendingCount, 1);

      completers[MarkdownSyntaxProfile.commonMarkCore]!.complete(
        _snapshotFor(revision: 9),
      );
      await _drainAsyncQueue();

      expect(started, [core, gfm]);
      completers[MarkdownSyntaxProfile.commonMarkGfm]!.complete(
        _snapshotFor(revision: 9),
      );
      await _drainAsyncQueue();

      expect(received, [9, 9]);
      expect(scheduler.staleDropCount, 0);
    });

    test('disposal clears pending and ignores late parse completion', () async {
      final started = <SyntaxParseRequest>[];
      final completers = <int, Completer<SyntaxSnapshot>>{};
      final received = <int>[];

      final scheduler = SyntaxParseScheduler(
        runParse: (request) {
          started.add(request);
          return (completers[request.revision] ??= Completer<SyntaxSnapshot>())
              .future;
        },
        onSnapshot: (snapshot) => received.add(snapshot.revision),
      );

      scheduler.schedule(const SyntaxParseRequest(revision: 1, text: 'a'));
      scheduler.schedule(const SyntaxParseRequest(revision: 2, text: 'ab'));

      scheduler.dispose();
      expect(scheduler.isDisposed, isTrue);
      expect(scheduler.pendingCount, 0);

      completers[1]!.complete(_snapshotFor(revision: 1));
      await _drainAsyncQueue();

      expect(started.map((r) => r.revision), [1]);
      expect(received, isEmpty);
      expect(scheduler.pendingCount, 0);
      expect(scheduler.inFlightCount, 0);
    });

    test('treats mismatched snapshot revision as stale', () async {
      final completer = Completer<SyntaxSnapshot>();
      final received = <int>[];

      final scheduler = SyntaxParseScheduler(
        runParse: (_) => completer.future,
        onSnapshot: (snapshot) => received.add(snapshot.revision),
      );
      addTearDown(scheduler.dispose);

      scheduler.schedule(const SyntaxParseRequest(revision: 5, text: 'hello'));

      completer.complete(_snapshotFor(revision: 4));
      await _drainAsyncQueue();

      expect(scheduler.staleDropCount, 1);
      expect(received, isEmpty);
    });
  });
}

SyntaxSnapshot _snapshotFor({required int revision}) {
  return SyntaxSnapshot.empty(revision: revision, textLength: 0);
}

Future<void> _drainAsyncQueue() async {
  await Future<void>.delayed(Duration.zero);
  await Future<void>.delayed(Duration.zero);
}
