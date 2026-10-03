import 'dart:async';

import 'package:flark/flark.dart';
import 'package:flark/render_model.dart';
import 'package:flark/session.dart';
import 'package:flark/src/parse/backend_ffi.dart' show FfiParseBackend;
import 'package:flark/src/session/shared_backend.dart';
import 'package:test/test.dart';

/// The source on which [_Recorded] faults.
const _trap = 'a source that traps';

/// A native parser that records how it is used. It faults on [_trap] as a
/// trapped Wasm instance does, and parses on afterwards as that instance does
/// once it has rebuilt itself.
final class _Recorded implements FlarkParseBackend {
  final _native = createParseBackend();
  int parses = 0;
  int disposals = 0;

  @override
  int get schemaVersion => _native.schemaVersion;

  @override
  RenderModel parse(String source) {
    parses++;
    if (source == _trap) {
      throw FlarkParseException.fromCode(FlarkParseException.faultCode);
    }
    return _native.parse(source);
  }

  @override
  void dispose() {
    disposals++;
    _native.dispose();
  }
}

/// A shared parser whose loads each add a [_Recorded] to [parsers].
(SharedBackend, List<_Recorded>) _shared() {
  final parsers = <_Recorded>[];
  return (
    SharedBackend(() async {
      final parser = _Recorded();
      parsers.add(parser);
      return parser;
    }),
    parsers,
  );
}

void main() {
  test('leases share one parser, which the last disposal disposes', () async {
    final (shared, parsers) = _shared();
    final leases = await Future.wait([
      for (var i = 0; i < 3; i++) shared.lease(),
    ]);
    expect(parsers, hasLength(1));
    for (final (i, lease) in leases.indexed) {
      expect(lease.backend.parse('# Heading $i').blockCount, 2);
    }
    expect(parsers.single.parses, 3);
    leases[0].dispose();
    leases[1].dispose();
    expect(parsers.single.disposals, 0);
    // A disposed lease refuses to parse, as a parser of its own would, while
    // the parser goes on serving the lease still held.
    expect(() => leases[0].backend.parse('late'), throwsStateError);
    expect(() => leases[0].backend.schemaVersion, throwsStateError);
    expect(leases[2].backend.parse('still held').blockCount, 2);
    leases[2].dispose();
    expect(parsers.single.disposals, 1);
    // The parser is not kept for reuse: the next lease loads a new one.
    final next = await shared.lease();
    expect(parsers, hasLength(2));
    expect(next.backend.parse('fresh').blockCount, 2);
    next.dispose();
    expect(parsers.last.disposals, 1);
  });

  test('leases asked for during the load wait for that one load', () async {
    final gate = Completer<FlarkParseBackend>();
    var loads = 0;
    final shared = SharedBackend(() {
      loads++;
      return gate.future;
    });
    final waiting = [shared.lease(), shared.lease(), shared.lease()];
    expect(loads, 1);
    final parser = _Recorded();
    gate.complete(parser);
    final leases = await Future.wait(waiting);
    expect(loads, 1);
    for (final lease in leases) {
      expect(lease.backend.parse('shared').blockCount, 2);
    }
    expect(parser.parses, 3);
    for (final lease in leases) {
      lease.dispose();
    }
    expect(parser.disposals, 1);
  });

  test(
    'a lease disposed as it arrives leaves the parser to those still waiting',
    () async {
      final gate = Completer<FlarkParseBackend>();
      final shared = SharedBackend(() => gate.future);
      // A holder disposed during loading releases its lease the moment the
      // lease arrives, which can be before another waiter has received its
      // own from the same load.
      final released = shared.lease().then((lease) => lease.dispose());
      final waiting = shared.lease();
      final parser = _Recorded();
      gate.complete(parser);
      await released;
      final lease = await waiting;
      expect(parser.disposals, 0);
      expect(lease.backend.parse('kept').blockCount, 2);
      lease.dispose();
      expect(parser.disposals, 1);
    },
  );

  test(
    'a failed load fails its waiters and is forgotten, so the next loads',
    () async {
      var loads = 0;
      final parsers = <_Recorded>[];
      final shared = SharedBackend(() {
        switch (++loads) {
          case 1:
            return Future.error(StateError('offline'));
          case 2:
            // A loader may also throw instead of failing its future.
            throw StateError('no module');
        }
        final parser = _Recorded();
        parsers.add(parser);
        return Future.value(parser);
      });
      final failed = [shared.lease(), shared.lease()];
      for (final lease in failed) {
        await expectLater(lease, throwsStateError);
      }
      expect(loads, 1);
      await expectLater(shared.lease(), throwsStateError);
      expect(loads, 2);
      final lease = await shared.lease();
      expect(loads, 3);
      expect(lease.backend.parse('recovered').blockCount, 2);
      // The failed leases hold nothing, so this one is the last.
      lease.dispose();
      expect(parsers.single.disposals, 1);
    },
  );

  test(
    'a lease retried as its load fails starts the load that later ones join',
    () async {
      var loads = 0;
      final parsers = <_Recorded>[];
      final shared = SharedBackend(() async {
        if (++loads == 1) throw StateError('offline');
        final parser = _Recorded();
        parsers.add(parser);
        return parser;
      });
      // A holder that retries as soon as it fails, as a listener calling a
      // reader's retry() does, asks again before every waiter on the failed
      // load has heard of the failure.
      final retried = shared.lease().catchError((Object _) => shared.lease());
      final failed = shared.lease();
      await expectLater(failed, throwsStateError);
      final first = await retried;
      final second = await shared.lease();
      expect(loads, 2);
      first.dispose();
      expect(parsers.single.disposals, 0);
      second.dispose();
      expect(parsers.single.disposals, 1);
    },
  );

  test('disposing a lent backend releases only its own lease, once', () async {
    final (shared, parsers) = _shared();
    final first = await shared.lease();
    final second = await shared.lease();
    first.backend.dispose();
    first.dispose();
    first.backend.dispose();
    expect(parsers.single.disposals, 0);
    expect(() => first.backend.parse('released'), throwsStateError);
    expect(second.backend.parse('held').blockCount, 2);
    second.dispose();
    expect(parsers.single.disposals, 1);
  });

  test(
    'a fault in one holder\'s parse leaves the shared parser to the others',
    () async {
      final (shared, parsers) = _shared();
      final faulting = await shared.lease();
      final other = await shared.lease();
      expect(
        () => faulting.backend.parse(_trap),
        throwsA(
          isA<FlarkParseException>().having(
            (error) => error.code,
            'code',
            FlarkParseException.faultCode,
          ),
        ),
      );
      expect(other.backend.parse('unaffected').blockCount, 2);
      expect(faulting.backend.parse('recovered').blockCount, 2);
      // A holder that replaces its lease after a fault is lent the same
      // parser while another holds it.
      faulting.dispose();
      final replacement = await shared.lease();
      expect(replacement.backend.parse('again').blockCount, 2);
      expect(parsers, hasLength(1));
      expect(parsers.single.disposals, 0);
      replacement.dispose();
      other.dispose();

      // Readers show the faulting text as source and keep rendering the rest.
      final faulted = FlarkReader('# One', backendLoader: shared.lease);
      final rendering = FlarkReader('# Two', backendLoader: shared.lease);
      await Future.wait([faulted.ready, rendering.ready]);
      faulted.update(_trap);
      expect(faulted.status, FlarkStatus.ready);
      expect(faulted.document!.sourceMode, isTrue);
      rendering.update('# Still rendered');
      expect(rendering.document!.sourceMode, isFalse);
      faulted.update('# Rendered again');
      expect(faulted.document!.sourceMode, isFalse);
      expect(parsers, hasLength(2));
      faulted.dispose();
      rendering.dispose();
      expect(parsers.last.disposals, 1);
    },
  );

  test(
    'readers and sessions share one parser and release it once each',
    () async {
      final (shared, parsers) = _shared();
      final released = <int>[];
      Future<FlarkBackendLease> Function() loader(int holder) => () async {
        final lease = await shared.lease();
        return FlarkBackendLease(lease.backend, () {
          released.add(holder);
          lease.dispose();
        });
      };
      final readers = [
        for (var i = 0; i < 50; i++)
          FlarkReader('# Reader $i', backendLoader: loader(i)),
      ];
      final sessions = [
        for (var i = 50; i < 100; i++)
          FlarkSession(markdown: 'Session $i', backendLoader: loader(i)),
      ];
      // A reader disposed before its lease arrives releases the lease on
      // arrival, without taking the parser from the others.
      FlarkReader('# Gone', backendLoader: loader(100)).dispose();
      await Future.wait([
        ...readers.map((reader) => reader.ready),
        ...sessions.map((session) => session.ready),
      ]);
      await Future<void>.delayed(Duration.zero);
      expect(parsers, hasLength(1));
      expect(released, [100]);
      readers.first.update('# Updated');
      expect(readers.first.document!.sourceMode, isFalse);
      expect(
        sessions.first.command(const InsertText('typed ')).changed,
        isTrue,
      );
      expect(sessions.first.state.markdown, 'typed Session 50');
      for (final reader in readers) {
        reader
          ..dispose()
          ..dispose();
      }
      for (final session in sessions.skip(1)) {
        session
          ..dispose()
          ..dispose();
      }
      expect(parsers.single.disposals, 0);
      expect(
        sessions.first.command(const InsertText('still ')).changed,
        isTrue,
      );
      sessions.first.dispose();
      expect(parsers.single.disposals, 1);
      expect(released..sort(), [for (var i = 0; i <= 100; i++) i]);
    },
  );

  test('on the Dart VM each lease still owns a native parser', () async {
    final first = await loadFlarkBackend();
    final second = await loadFlarkBackend();
    expect(first.backend, isA<FfiParseBackend>());
    expect(second.backend, isA<FfiParseBackend>());
    expect(identical(first.backend, second.backend), isFalse);
    first.dispose();
    expect(() => first.backend.parse('disposed'), throwsStateError);
    expect(second.backend.parse('own parser').blockCount, 2);
    second.dispose();
  });
}
