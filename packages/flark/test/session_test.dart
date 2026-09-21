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
      gate.complete(
        FlarkBackendLease(backend, () => (backend as dynamic).dispose()),
      );
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
          (backend as dynamic).dispose();
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
