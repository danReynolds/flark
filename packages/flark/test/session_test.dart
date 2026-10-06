import 'dart:async';
import 'package:flark/flark.dart';
import 'package:flark/session.dart';
import 'package:test/test.dart';

final class Controller with FlarkActions {
  Controller({
    String markdown = '',
    Future<FlarkBackendLease> Function()? loader,
  }) : session = FlarkSession(markdown: markdown, backendLoader: loader);
  @override
  final FlarkSession session;
}

void main() {
  test('resource capabilities and update reject unsupported targets', () async {
    final session = FlarkSession(markdown: 'first\n\nsecond');
    await session.ready;
    addTearDown(session.dispose);
    session.selectAll();
    expect(session.state.link.canSet, isFalse);
    final before = session.state;
    expect(
      session.updateImage('https://example.com/a.png').reason,
      FlarkEditRejection.unsupportedEdit,
    );
    expect(session.state.revision, before.revision);
    expect(session.state.canUndo, isFalse);
    session.loadMarkdown('`inline`');
    session.command(const SetSelection(3, 3));
    expect(session.state.link.canSet, isFalse);
    session.loadMarkdown('![alt](old.png)');
    session.command(const SetSelection(3, 3));
    expect(session.updateImage('new.png').changed, isTrue);
    expect(session.state.markdown, contains('new.png'));
  });

  test(
    'Undo during a first composition undoes it, as the editor does',
    () async {
      // Undo commits an open composition and takes it back, also when no
      // step came before it: then the composition is the step to undo.
      final session = FlarkSession(markdown: 'abc');
      await session.ready;
      addTearDown(session.dispose);
      final editor = session.engine!;
      editor.apply(const SetSelection(3, 3));
      editor.beginComposition();
      editor.apply(const InsertText('k'));
      expect(session.state.canUndo, isFalse);
      expect(session.command(const Undo()).changed, isTrue);
      expect((session.state.markdown, editor.composing), ('abc', false));
      expect(session.command(const Redo()).changed, isTrue);
      expect(session.state.markdown, 'abck');
      expect(session.command(const Redo()).outcome, FlarkEditOutcome.unchanged);
    },
  );

  test(
    'latest loading seed, save stream, reset and undoable replacement',
    () async {
      final gate = Completer<FlarkBackendLease>();
      final c = Controller(markdown: 'old', loader: () => gate.future);
      addTearDown(c.session.dispose);
      final edits = <String>[];
      c.changes.listen(edits.add);
      expect(c.insertText('no').reason, FlarkEditRejection.notReady);
      c.loadMarkdown('fetched');
      final revision = c.state.revision;
      final backend = createParseBackend();
      gate.complete(FlarkBackendLease(backend, backend.dispose));
      await c.ready;
      expect(c.markdown, 'fetched');
      expect(c.state.revision, revision);
      c.setSelection(7, 7);
      c.insertText('!');
      c.undo();
      c.redo();
      expect(edits, ['fetched!', 'fetched', 'fetched!']);
      final beforeLoad = c.state.revision;
      c.loadMarkdown('fetched!');
      expect(c.state.revision, greaterThan(beforeLoad));
      expect(c.state.canUndo, isFalse);
      expect(c.state.selection.extent, 0);
      expect(edits.length, 3);
      c.replaceMarkdown('replacement');
      expect(c.state.selection.extent, 11);
      c.undo();
      expect(c.markdown, 'fetched!');
    },
  );
  test(
    'failed readiness, joined retries, disposal releases late load',
    () async {
      var calls = 0, released = 0;
      final gate = Completer<FlarkBackendLease>();
      final c = Controller(
        loader: () =>
            ++calls == 1 ? Future.error(StateError('offline')) : gate.future,
      );
      await expectLater(c.ready, throwsStateError);
      expect(c.state.status, FlarkStatus.failed);
      final retry = c.retryLoading();
      expect(identical(retry, c.retryLoading()), isTrue);
      final failedReady = expectLater(retry, throwsStateError);
      c.session.dispose();
      c.session.dispose();
      await failedReady;
      final backend = createParseBackend();
      gate.complete(
        FlarkBackendLease(backend, () {
          released++;
          backend.dispose();
        }),
      );
      await Future<void>.delayed(Duration.zero);
      expect(released, 1);
      expect(c.state.status, FlarkStatus.disposed);
      expect(c.undo().reason, FlarkEditRejection.disposed);
    },
  );
  test('snapshots, typing state, revision guards and attachment', () async {
    final c = Controller(markdown: 'hello');
    addTearDown(c.session.dispose);
    await c.ready;
    final old = c.state;
    c.toggleStyle(FlarkStyle.bold);
    expect(old.styles.bold.isOn, isFalse);
    expect(c.state.styles.bold.isOn, isTrue);
    expect(
      c.insertText('stale', expectedRevision: old.revision).reason,
      FlarkEditRejection.staleRevision,
    );
    c.insertText('new');
    expect(c.markdown, '**new**hello');
    final owner = Object();
    c.session.attach(owner);
    expect(() => c.session.attach(Object()), throwsStateError);
    c.session.detach(owner);
    c.session.attach(Object());
  });
  test(
    'exact splice preserves directional selection and rejects surrogate split',
    () async {
      final c = Controller(markdown: 'abc def ghi');
      addTearDown(c.session.dispose);
      await c.ready;
      c.setSelection(10, 8);
      c.replaceSourceRange(start: 0, end: 3, markdown: 'ABCD');
      expect(c.markdown, 'ABCD def ghi');
      expect(c.state.selection.base, 11);
      expect(c.state.selection.extent, 9);
      c.undo();
      expect(c.markdown, 'abc def ghi');
      expect(c.state.selection.base, 10);
      c.loadMarkdown('a😀b');
      expect(
        c.replaceSourceRange(start: 2, end: 3, markdown: 'x').reason,
        FlarkEditRejection.invalidSource,
      );
      expect(c.markdown, 'a😀b');
      expect(
        c.replaceSourceRange(start: -1, end: 0, markdown: 'x').accepted,
        isFalse,
      );
      expect(
        c.replaceSourceRange(start: 3, end: 2, markdown: 'x').accepted,
        isFalse,
      );
      c.replaceSourceRange(start: 1, end: 3, markdown: 'X');
      expect(c.markdown, 'aXb');
    },
  );
  test('rejected exact edits do not commit active composition', () async {
    final c = Controller(markdown: 'abc');
    addTearDown(c.session.dispose);
    await c.ready;
    final engine = c.session.engine!;
    final stale = c.state.revision;
    engine.beginComposition();
    engine.apply(const InsertText('x'));
    expect(
      c
          .replaceSourceRange(
            start: 0,
            end: 1,
            markdown: 'z',
            expectedRevision: stale,
          )
          .reason,
      FlarkEditRejection.staleRevision,
    );
    expect(engine.composing, isTrue);
    expect(
      c.replaceSourceRange(start: 0, end: 1, markdown: '\ud800').reason,
      FlarkEditRejection.invalidSource,
    );
    expect(engine.composing, isTrue);
    expect(engine.history.canUndo, isFalse);
    engine.cancelComposition();
    expect(c.markdown, 'abc');
  });
  test('programmatic select all has deterministic scope', () async {
    final c = Controller(markdown: '```\ncode\n```\n\nafter');
    addTearDown(c.session.dispose);
    await c.ready;
    c.setSelection(5, 5);
    c.selectAll();
    expect(c.state.selection.start, 0);
    expect(c.state.selection.end, c.markdown.length);
    c.setSelection(5, 5);
    c.selectAll(scope: FlarkSelectionScope.codeBlock);
    expect(c.state.selection.start, 4);
    expect(c.state.selection.end, 8);
  });
  test('source mode asked for stays when the document shrinks', () async {
    // A document past the live tier shows its source anyway, and asking for
    // rendered mode there changes nothing. Asking for source mode is still a
    // request: when the document shrinks to fit the tier, it stays.
    final session = FlarkSession(markdown: 'x' * 20, syncLimit: 10);
    addTearDown(session.dispose);
    await session.ready;
    final editor = session.engine!;
    expect(editor.sourceMode, isTrue);
    final revision = session.state.revision;
    expect(session.setSourceMode(false).outcome, FlarkEditOutcome.unchanged);
    expect(session.state.revision, revision);
    expect(session.setSourceMode(true).changed, isTrue);
    expect(session.replaceSourceRange(0, 20, 'x').changed, isTrue);
    expect(editor.sourceMode, isTrue);
    expect(session.setSourceMode(true).outcome, FlarkEditOutcome.unchanged);
    expect(session.setSourceMode(false).changed, isTrue);
    expect(editor.sourceMode, isFalse);
  });

  test('rejected callable commands preserve IME undo state', () async {
    final c = Controller(markdown: 'abc');
    addTearDown(c.session.dispose);
    await c.ready;
    final e = c.session.engine!;
    e.beginComposition();
    e.apply(const InsertText('x'));
    expect(c.insertText('\ud800').reason, FlarkEditRejection.invalidSource);
    expect(e.composing, isTrue);
    expect(e.history.canUndo, isFalse);
    expect(c.insertImage('').accepted, isFalse);
    expect(e.composing, isTrue);
    c.insertText('valid');
    expect(e.composing, isFalse);
    c.undo();
    expect(c.markdown, 'xabc');
    c.undo();
    expect(c.markdown, 'abc');
  });

  test('loadMarkdown rejects text the editor cannot hold', () async {
    final c = Controller(markdown: 'kept');
    addTearDown(c.session.dispose);
    await c.ready;
    final revision = c.state.revision;
    for (final (text, reason) in [
      ('half an emoji \uD83D', FlarkEditRejection.invalidSource),
      ('classic\rmac', FlarkEditRejection.invalidSource),
      ('x' * (1024 * 1024 + 1), FlarkEditRejection.sourceLimit),
    ]) {
      expect(c.loadMarkdown(text).reason, reason);
    }
    expect(c.markdown, 'kept');
    expect(c.state.revision, revision);
  });

  test('a load refused while loading leaves the seed to open', () async {
    final gate = Completer<FlarkBackendLease>();
    final c = Controller(markdown: 'seed', loader: () => gate.future);
    addTearDown(c.session.dispose);
    expect(
      c.loadMarkdown('classic\rmac').reason,
      FlarkEditRejection.invalidSource,
    );
    final backend = createParseBackend();
    gate.complete(FlarkBackendLease(backend, backend.dispose));
    await c.ready;
    expect(c.state.status, FlarkStatus.ready);
    expect(c.markdown, 'seed');
  });

  test('a refused seed fails the session, and other content opens', () async {
    for (final (seed, error) in [
      ('classic\rmac', isFormatException),
      ('draft \uD83D', isFormatException),
      ('x' * (1024 * 1024 + 1), isArgumentError),
    ]) {
      var loads = 0;
      final c = Controller(
        markdown: seed,
        loader: () {
          loads++;
          return loadFlarkBackend();
        },
      );
      addTearDown(c.session.dispose);
      await expectLater(c.ready, throwsA(error));
      expect(c.state.status, FlarkStatus.failed);
      expect(c.state.error, error);
      // Retrying the same content fails the same way, without a parser.
      await expectLater(c.retryLoading(), throwsA(error));
      expect(loads, 0);
      expect(c.loadMarkdown('# Fixed').changed, isTrue);
      await c.retryLoading();
      expect(c.state.status, FlarkStatus.ready);
      expect(c.markdown, '# Fixed');
      expect(loads, 1);
    }
  });

  test('a listener may dispose a session whose retry is refused', () async {
    final s = FlarkSession(markdown: 'classic\rmac');
    await expectLater(s.ready, throwsFormatException);
    s.addListener(() {
      if (s.state.status == FlarkStatus.failed) s.dispose();
    });
    // Disposing completes the retry's attempt before the refusal does.
    expect(
      () => unawaited(s.retryLoading().catchError((Object _) {})),
      returnsNormally,
    );
  });

  test('limits no editor can apply are refused at construction', () {
    for (final create in [
      () => FlarkSession(syncLimit: 1024 * 1024 + 1),
      () => FlarkSession(syncLimit: -1),
      () => FlarkSession(liveLimits: const FlarkLiveLimits(lines: 0)),
      () => FlarkSession(liveLimits: const FlarkLiveLimits(containerDepth: -1)),
    ]) {
      expect(create, throwsArgumentError);
    }
    // The writable limit itself is the widest live limit.
    FlarkSession(syncLimit: 1024 * 1024).dispose();
  });

  test(
    'a throwing listener neither forks the document nor silences others',
    () async {
      final c = Controller(markdown: 'abc');
      addTearDown(c.session.dispose);
      await c.ready;
      final saved = <String>[];
      c.changes.listen(saved.add);
      // An open composition is where a failed command restores its snapshot.
      final engine = c.session.engine!;
      engine.beginComposition();
      engine.apply(const InsertText('x'));
      var heard = 0;
      void failing() => throw StateError('listener bug');
      c.session.addListener(failing);
      c.session.addListener(() => heard++);
      final errors = <Object>[];
      late FlarkEditResult result;
      runZonedGuarded(() {
        result = c.insertText('y');
      }, (error, _) => errors.add(error));
      c.session.removeListener(failing);
      expect(result.changed, isTrue);
      expect(errors.single, isStateError);
      expect(heard, 1);
      expect(engine.source, 'xyabc');
      expect(c.markdown, engine.source);
      expect(saved.last, engine.source);
    },
  );

  test('save callbacks can edit without recursive stream failure', () async {
    final c = Controller();
    addTearDown(c.session.dispose);
    await c.ready;
    final saved = <String>[];
    c.changes.listen((text) {
      saved.add(text);
      if (text == 'first') c.replaceMarkdown('second');
    });
    c.replaceMarkdown('first');
    expect(saved, ['first', 'second']);
    expect(c.markdown, 'second');
  });
}
